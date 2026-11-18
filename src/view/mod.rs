//! Drawing: each screen reads the state and paints it, and none of them does I/O.
mod list;

use crate::app::{App, Fit, Status};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vtui::theme::Theme;

const SPINNER: [&str; 4] = ["|", "/", "-", "\\"];
const HINTS: &str = "? keys  enter open  m mark  F fix  / search  q quit";

fn status_line<'a>(app: &App, theme: &Theme) -> Line<'a> {
    match &app.status {
        Status::Error(message) => Line::from(Span::styled(format!("! {message}"), Style::default().fg(theme.danger))),
        Status::Info(message) => Line::from(Span::styled(message.clone(), Style::default().fg(theme.muted))),
        Status::Idle => Line::from(Span::styled(HINTS, Style::default().fg(theme.faded))),
    }
}

/// Paints the screen the state is on and says what the detail screen measured.
pub fn draw(frame: &mut Frame<'_>, app: &App, theme: &Theme) -> Fit {
    let [header, body, footer] = Layout::vertical([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());
    let spinner = if app.is_busy() { SPINNER[app.spinner % SPINNER.len()] } else { " " };
    frame.render_widget(Paragraph::new(list::header(app, theme, spinner)), header);
    list::draw(frame, body, app, theme);
    frame.render_widget(Paragraph::new(status_line(app, theme)), footer);
    Fit::default()
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::app::{App, Settings};
    use crate::langsmith::ThreadsPage;
    use crate::marks::Marks;
    use crate::model::{FeedbackKey, ProjectId, Thread, ThreadId};
    use chrono::{DateTime, Utc};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Cell;
    use vtui::theme::Theme;

    pub(crate) fn settings(has_key: bool) -> Settings {
        Settings {
            project: "app-production".into(),
            since_label: "7d".into(),
            fix_program: "claude".into(),
            repo_dir: "/repo".into(),
            url_template: "https://smith.example/{trace_id}".into(),
            org_id: "org".into(),
            has_key,
        }
    }

    pub(crate) fn now() -> DateTime<Utc> {
        "2026-03-02T12:00:00Z".parse().unwrap()
    }

    pub(crate) fn thread(id: &str, input: &str, errored: u32, feedback: bool) -> Thread {
        Thread {
            id: ThreadId(id.into()),
            started_at: Some(now()),
            last_active_at: Some(now() - chrono::Duration::hours(2)),
            last_trace_id: Some("tr".into()),
            first_input: Some(input.into()),
            turn_count: 3,
            errored_turns: errored,
            last_error: None,
            feedback: if feedback { vec![FeedbackKey { key: "score".into(), n: 1, avg: Some(1.0) }] } else { Vec::new() },
        }
    }

    pub(crate) fn app_with(threads: Vec<Thread>) -> App {
        let mut app = App::new(settings(true), Marks::default(), now());
        app.start();
        let message = crate::app::Msg::Project { generation: 1, result: Ok(ProjectId("p".into())) };
        let (app, _) = crate::app::update(app, message);
        let page = ThreadsPage { threads, next_cursor: Some("more".into()) };
        crate::app::update(app, crate::app::Msg::Threads { generation: 1, result: Ok(page) }).0
    }

    pub(crate) fn render(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| drop(super::draw(frame, app, &Theme::default()))).unwrap();
        let buffer = terminal.backend().buffer();
        buffer
            .content()
            .chunks(usize::from(width))
            .map(|row| row.iter().map(Cell::symbol).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
