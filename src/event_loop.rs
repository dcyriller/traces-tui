//! The loop that owns the terminal: it feeds keys and fetch results to `update` and runs the effects it returns.
use crate::app::{App, Effect, Fit, FixDone, Msg, Settings, update};
use crate::config::parse_since;
use crate::ctx::Ctx;
use crate::fix;
use crate::keys::translate;
use crate::langsmith::{ApiError, Client};
use crate::marks::Marks;
use crate::model::ThreadId;
use crate::view;
use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use crossterm::event::{Event, EventStream, KeyEventKind};
use futures_util::StreamExt;
use ratatui::DefaultTerminal;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::time::{Duration as TickDuration, sleep};
use vtui::screen::Screen;
use vtui::theme::Theme;

const SPINNER_TICK_MS: u64 = 250;

struct Runner {
    client: Option<Arc<Client>>,
    sender: UnboundedSender<Msg>,
    project: String,
    since: Duration,
    marks_path: PathBuf,
}

impl Runner {
    fn spawn<T, Fut>(&self, call: impl FnOnce(Arc<Client>) -> Fut, wrap: impl FnOnce(Result<T, String>) -> Msg + Send + 'static)
    where
        Fut: Future<Output = Result<T, ApiError>> + Send + 'static,
        T: Send + 'static,
    {
        let Some(client) = &self.client else { return };
        let future = call(Arc::clone(client));
        let sender = self.sender.clone();
        tokio::spawn(async move {
            let result = future.await.map_err(|error| {
                tracing::warn!(%error, "langsmith call failed");
                error.to_string()
            });
            if sender.send(wrap(result)).is_err() {
                tracing::warn!("the loop is gone, dropping a LangSmith answer");
            }
        });
    }

    fn fail(&self, message: String) {
        if self.sender.send(Msg::Failure(message)).is_err() {
            tracing::warn!("the loop is gone, dropping a failure message");
        }
    }

    fn run(&self, effect: Effect, marks: &Marks) {
        match effect {
            Effect::ResolveProject { generation } => {
                let name = self.project.clone();
                self.spawn(|client| async move { client.resolve_project(&name).await }, move |result| Msg::Project { generation, result });
            }
            Effect::LoadThreads { generation, project, cursor } => {
                let since = Utc::now() - self.since;
                self.spawn(
                    |client| async move { client.list_threads(&project, since, cursor.as_deref()).await },
                    move |result| Msg::Threads { generation, result },
                );
            }
            Effect::LoadTurns { generation, project, thread } => {
                self.spawn(
                    |client| async move { client.list_turns(&project, &thread).await },
                    move |result| Msg::Turns { generation, result },
                );
            }
            Effect::SaveMarks => {
                if let Err(error) = marks.save(&self.marks_path) {
                    self.fail(format!("{error:#}"));
                }
            }
            Effect::OpenUrl(url) => self.open(&url),
            Effect::Fix(_) | Effect::Quit => {}
        }
    }

    fn open(&self, url: &str) {
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        match tokio::process::Command::new(opener).arg(url).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn()
        {
            Ok(mut child) => {
                tokio::spawn(async move {
                    if let Err(error) = child.wait().await {
                        tracing::warn!(%error, "waiting for the browser opener failed");
                    }
                });
            }
            Err(error) => self.fail(format!("{opener}: {error}")),
        }
    }
}

fn run_fix(ctx: &Ctx, marks: &Marks, threads: &[ThreadId]) -> FixDone {
    let project = ctx.config.project.as_str();
    let chosen: Vec<_> = threads.iter().filter_map(|thread| marks.get(project, thread)).collect();
    let outcome = fix::plan(&ctx.config, project, &chosen).and_then(|plan| {
        for placeholder in &plan.unknown_placeholders {
            tracing::warn!(placeholder, "unknown placeholder in the prompt template");
        }
        fix::run(&plan)
    });
    FixDone { threads: threads.to_vec(), outcome: outcome.map_err(|error| format!("{error:#}")), finished_at: Utc::now() }
}

fn messages_of(app: &App, event: Event) -> Vec<Msg> {
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => translate(&app.mode, app.help.is_some(), key).into_iter().collect(),
        Event::Paste(text) => vec![Msg::Paste(text)],
        _ => Vec::new(),
    }
}

fn settings_of(ctx: &Ctx) -> Settings {
    let program = shell_words::split(&ctx.config.fix_command).ok().and_then(|words| words.into_iter().next());
    Settings {
        project: ctx.config.project.clone(),
        since_label: ctx.config.since.clone(),
        fix_program: program.unwrap_or_else(|| ctx.config.fix_command.clone()),
        repo_dir: ctx.config.repo_dir.display().to_string(),
        url_template: ctx.config.web_url_template.clone(),
        org_id: ctx.config.org_id.clone(),
        has_key: ctx.api_key().is_ok(),
    }
}

/// Runs the TUI until the user quits, giving the terminal back on every way out.
pub async fn run(ctx: Ctx) -> Result<()> {
    let marks = Marks::load(&ctx.marks_path)?;
    let since = parse_since(&ctx.config.since)?;
    let theme =
        Theme::named(&ctx.config.theme).with_context(|| format!("unknown theme {}, try one of {:?}", ctx.config.theme, Theme::NAMES))?;
    let client = match ctx.api_key() {
        Ok(key) => Some(Arc::new(Client::new(&ctx.config.api_url, key)?)),
        Err(_) => None,
    };
    let (sender, mut incoming) = mpsc::unbounded_channel();
    let runner = Runner { client, sender, project: ctx.config.project.clone(), since, marks_path: ctx.marks_path.clone() };
    let mut app = App::new(settings_of(&ctx), marks, Utc::now());
    let first = app.start();
    for effect in first {
        runner.run(effect, &app.marks);
    }
    let (mut terminal, screen) = Screen::enter_without_mouse();
    let outcome = event_loop(&mut terminal, &screen, app, &runner, &mut incoming, &ctx, &theme).await;
    screen.leave();
    outcome
}

async fn event_loop(
    terminal: &mut DefaultTerminal,
    screen: &Screen,
    mut app: App,
    runner: &Runner,
    incoming: &mut UnboundedReceiver<Msg>,
    ctx: &Ctx,
    theme: &Theme,
) -> Result<()> {
    let mut events = EventStream::new();
    let mut queue: VecDeque<Msg> = VecDeque::new();
    while !app.should_quit() {
        app.now = Utc::now();
        let mut fit = Fit::default();
        terminal.draw(|frame| fit = view::draw(frame, &app, theme))?;
        app.fit(fit);
        if queue.is_empty() {
            tokio::select! {
                Some(event) = events.next() => queue.extend(messages_of(&app, event?)),
                Some(msg) = incoming.recv() => queue.push_back(msg),
                () = sleep(TickDuration::from_millis(SPINNER_TICK_MS)), if app.is_busy() => queue.push_back(Msg::Tick),
            }
        }
        while let Some(msg) = queue.pop_front() {
            let (next, effects) = update(app, msg);
            app = next;
            for effect in effects {
                let Effect::Fix(threads) = effect else {
                    runner.run(effect, &app.marks);
                    continue;
                };
                drop(events);
                let done = tokio::task::block_in_place(|| screen.lend(terminal, || run_fix(ctx, &app.marks, &threads)));
                events = EventStream::new();
                queue.push_back(Msg::Fix(done));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::marks::tests::mark;

    fn ctx_running(command: &str, dir: &std::path::Path) -> Ctx {
        let cli = <crate::cli::Cli as clap::Parser>::parse_from(["traces", "--config", dir.join("none.toml").to_str().unwrap()]);
        let mut ctx = Ctx::build(&cli, crate::ctx::Env { home: dir.to_path_buf(), ..Default::default() }).unwrap();
        ctx.config = Config { fix_command: command.into(), repo_dir: dir.to_path_buf(), ..Config::default() };
        ctx
    }

    fn marks_of(thread: &str) -> Marks {
        let mut marks = Marks::default();
        marks.upsert(mark(thread, "n"));
        marks
    }

    #[test]
    fn a_command_that_exits_zero_is_a_success_carrying_the_threads() {
        let dir = tempfile::tempdir().unwrap();
        let done = run_fix(&ctx_running("true", dir.path()), &marks_of("a"), &[ThreadId("a".into())]);
        assert_eq!((done.outcome, done.threads), (Ok(()), vec![ThreadId("a".into())]));
    }

    #[test]
    fn a_command_that_exits_non_zero_is_an_error_naming_it() {
        let dir = tempfile::tempdir().unwrap();
        let done = run_fix(&ctx_running("false", dir.path()), &marks_of("a"), &[ThreadId("a".into())]);
        assert!(done.outcome.unwrap_err().starts_with("false exited with"));
    }

    #[test]
    fn a_missing_program_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let done = run_fix(&ctx_running("no-such-program-xyz", dir.path()), &marks_of("a"), &[ThreadId("a".into())]);
        assert!(done.outcome.unwrap_err().contains("not found on PATH"));
    }
}
