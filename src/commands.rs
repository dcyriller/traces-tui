//! The subcommands that skip the TUI.
use crate::cli::{FixArgs, MarksArgs};
use crate::ctx::Ctx;
use crate::fix::{launch, plan};
use crate::langsmith::Client;
use crate::marks::{Mark, MarkStatus, Marks};
use crate::model::ThreadId;
use anyhow::{Context, Result, bail};
use chrono::Utc;
use std::io::{BufRead, Write};

fn status_name(status: MarkStatus) -> &'static str {
    match status {
        MarkStatus::Todo => "todo",
        MarkStatus::Sent => "sent",
    }
}

fn selected<'a>(marks: &'a Marks, project: &str, status: Option<&str>) -> Vec<&'a Mark> {
    marks.all().iter().filter(|mark| mark.project == project && status.is_none_or(|wanted| status_name(mark.status) == wanted)).collect()
}

/// Prints the marks of the project, as a table or as JSON.
pub fn marks(ctx: &Ctx, args: &MarksArgs, out: &mut impl Write) -> Result<()> {
    let store = Marks::load(&ctx.marks_path)?;
    let chosen = selected(&store, &ctx.config.project, args.status.as_deref());
    if args.json {
        writeln!(out, "{}", serde_json::to_string_pretty(&chosen)?)?;
        return Ok(());
    }
    for mark in chosen {
        let note = if mark.note.is_empty() { "(no note)" } else { &mark.note };
        writeln!(
            out,
            "{}  {}  {}  {}",
            status_name(mark.status),
            mark.thread_id,
            mark.marked_at.format("%Y-%m-%d"),
            note.lines().next().unwrap_or(note)
        )?;
    }
    Ok(())
}

fn confirmed(question: &str, input: &mut impl BufRead, out: &mut impl Write) -> Result<bool> {
    write!(out, "{question} [y/N] ")?;
    out.flush()?;
    let mut answer = String::new();
    input.read_line(&mut answer)?;
    Ok(answer.trim().eq_ignore_ascii_case("y"))
}

/// Launches the fix command on the project's marks, or prints what it would run.
pub fn fix(ctx: &Ctx, args: &FixArgs, input: &mut impl BufRead, out: &mut impl Write) -> Result<()> {
    let mut store = Marks::load(&ctx.marks_path)?;
    let project = ctx.config.project.as_str();
    let chosen = if args.all { selected(&store, project, None) } else { store.todo(project) };
    if chosen.is_empty() {
        bail!("no marked threads to fix in {project}");
    }
    let plan = plan(&ctx.config, project, &chosen)?;
    for placeholder in &plan.unknown_placeholders {
        writeln!(out, "warning: the prompt template has an unknown placeholder {placeholder}")?;
    }
    if args.dry_run {
        let program = &plan.argv[..plan.argv.len() - 1];
        writeln!(out, "cd {}\n{}\n\n{}", plan.cwd.display(), shell_words::join(program), plan.prompt)?;
        return Ok(());
    }
    let question = format!("Launch {} on {} threads in {}?", plan.argv[0], chosen.len(), plan.cwd.display());
    if !args.yes && !confirmed(&question, input, out)? {
        return Ok(());
    }
    let threads: Vec<ThreadId> = chosen.iter().map(|mark| mark.thread_id.clone()).collect();
    let status = launch(&plan)?;
    if !status.success() {
        bail!("{} exited with {status}", plan.argv[0]);
    }
    store.mark_sent(project, &threads, Utc::now());
    store.save(&ctx.marks_path)
}

/// Prints the link LangSmith gives for the latest turn of a thread.
pub async fn url(ctx: &Ctx, thread_id: &str, out: &mut impl Write) -> Result<()> {
    if ctx.config.org_id.is_none() {
        bail!("set org_id in the config file to get links");
    }
    let client = Client::new(&ctx.config.api_url, ctx.api_key()?)?;
    let project = client.resolve_project(&ctx.config.project).await?;
    let turns = client.list_turns(&project, &ThreadId(thread_id.to_owned())).await?;
    let latest = turns.turns.last().context("the thread has no turns")?;
    writeln!(out, "{}", client.read_run_url(&project, &latest.trace_id).await?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::marks::tests::mark;

    fn ctx_with_marks(dir: &std::path::Path, notes: &[(&str, &str)]) -> Ctx {
        let cli = <crate::cli::Cli as clap::Parser>::parse_from(["traces", "--config", dir.join("none.toml").to_str().unwrap()]);
        let env = crate::ctx::Env { home: dir.to_path_buf(), xdg_data_home: Some(dir.to_path_buf()), ..Default::default() };
        let mut ctx = Ctx::build(&cli, env).unwrap();
        ctx.config = Config { fix_command: "true".into(), repo_dir: dir.to_path_buf(), ..Config::default() };
        let mut store = Marks::default();
        for (thread, note) in notes {
            store.upsert(mark(thread, note));
        }
        store.save(&ctx.marks_path).unwrap();
        ctx
    }

    #[test]
    fn marks_json_prints_the_stored_marks() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ctx_with_marks(dir.path(), &[("a", "wrong country")]);
        let mut out = Vec::new();
        marks(&ctx, &MarksArgs { json: true, status: None }, &mut out).unwrap();
        let printed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(printed[0]["thread_id"], "a");
        assert_eq!(printed[0]["note"], "wrong country");
    }

    #[tokio::test]
    async fn url_asks_for_an_org_id_first() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ctx_with_marks(dir.path(), &[]);
        let err = url(&ctx, "t", &mut Vec::new()).await.unwrap_err();
        assert!(err.to_string().contains("org_id"), "{err}");
    }

    #[test]
    fn marks_filters_by_status() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ctx_with_marks(dir.path(), &[("a", "n")]);
        let mut out = Vec::new();
        marks(&ctx, &MarksArgs { json: false, status: Some("sent".into()) }, &mut out).unwrap();
        assert_eq!(out, b"");
    }

    #[test]
    fn fix_dry_run_prints_the_prompt_and_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ctx_with_marks(dir.path(), &[("a", "wrong country")]);
        let mut out = Vec::new();
        fix(&ctx, &FixArgs { dry_run: true, yes: false, all: false }, &mut std::io::empty(), &mut out).unwrap();
        let printed = String::from_utf8(out).unwrap();
        assert!(printed.contains("wrong country") && printed.contains("these 1 conversations"));
        assert_eq!(Marks::load(&ctx.marks_path).unwrap().todo("app-production").len(), 1);
    }

    #[test]
    fn fix_with_yes_flips_the_marks_to_sent_and_without_it_asks_first() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ctx_with_marks(dir.path(), &[("a", "n")]);
        let declined = FixArgs { dry_run: false, yes: false, all: false };
        fix(&ctx, &declined, &mut "n\n".as_bytes(), &mut Vec::new()).unwrap();
        assert_eq!(Marks::load(&ctx.marks_path).unwrap().todo("app-production").len(), 1);
        fix(&ctx, &FixArgs { yes: true, ..declined }, &mut std::io::empty(), &mut Vec::new()).unwrap();
        assert_eq!(Marks::load(&ctx.marks_path).unwrap().todo("app-production").len(), 0);
    }
}
