//! The thread list: the header, the rows and the messages that stand in for rows.
use crate::app::{App, Filter, Loading, Status};
use crate::marks::MarkStatus;
use crate::model::Thread;
use chrono::{DateTime, Local, Utc};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vtui::text::truncate;
use vtui::theme::Theme;

const TIME_WIDTH: usize = 9;
const TURNS_WIDTH: usize = 4;
const MARKERS_WIDTH: usize = 5;
const FIXED_WIDTH: usize = 1 + TIME_WIDTH + 1 + 1 + TURNS_WIDTH + MARKERS_WIDTH;

fn filter_name(filter: Filter) -> &'static str {
    match filter {
        Filter::All => "all",
        Filter::Marked => "marked",
        Filter::Todo => "todo",
    }
}

/// The line above the list: project, how many rows are loaded and the active filter and search.
pub(super) fn header<'a>(app: &App, theme: &Theme, spinner: &'a str) -> Line<'a> {
    let more = if app.next_cursor.is_some() { "  more" } else { "" };
    let search = if app.search.is_empty() { String::new() } else { format!("  /{}", app.search) };
    let text = format!(" {}  {} loaded  filter {}{search}{more}", app.settings.project, app.threads.len(), filter_name(app.filter));
    Line::from(vec![
        Span::styled(text, Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(spinner, Style::default().fg(theme.muted)),
    ])
}

/// `2h ago` inside a day, the local date after that.
fn time_label(at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    let Some(at) = at else { return "-".to_owned() };
    let elapsed = now - at;
    match () {
        () if elapsed.num_minutes() < 1 => "now".to_owned(),
        () if elapsed.num_hours() < 1 => format!("{}m ago", elapsed.num_minutes()),
        () if elapsed.num_hours() < 24 => format!("{}h ago", elapsed.num_hours()),
        () => at.with_timezone(&Local).format("%Y-%m-%d").to_string(),
    }
}

fn mark_span(app: &App, thread: &Thread, theme: &Theme) -> Span<'static> {
    match app.mark_of(thread).map(|mark| mark.status) {
        Some(MarkStatus::Todo) => Span::styled("*", Style::default().fg(theme.accent)),
        Some(MarkStatus::Sent) => Span::styled("v", Style::default().fg(theme.faded)),
        None => Span::raw(" "),
    }
}

fn row<'a>(app: &App, thread: &Thread, is_selected: bool, width: usize, theme: &Theme) -> Line<'a> {
    let message = thread.first_input.as_deref().unwrap_or("(no message)").replace('\n', " ");
    let room = width.saturating_sub(FIXED_WIDTH);
    let message = truncate(&message, room);
    let bar = if is_selected { Span::styled("▎", Style::default().fg(theme.accent)) } else { Span::raw(" ") };
    let errored = if thread.errored_turns > 0 { Span::styled("E", Style::default().fg(theme.danger)) } else { Span::raw(" ") };
    let feedback = if thread.has_feedback() { Span::styled("F", Style::default().fg(theme.warn)) } else { Span::raw(" ") };
    let weight = if is_selected { Modifier::BOLD } else { Modifier::empty() };
    Line::from(vec![
        bar,
        Span::styled(format!("{:<TIME_WIDTH$}", time_label(thread.last_active_at, app.now)), Style::default().fg(theme.muted)),
        Span::raw(" "),
        Span::styled(format!("{message:<room$}"), Style::default().add_modifier(weight)),
        Span::styled(format!(" {:>3}", thread.turn_count), Style::default().fg(theme.muted)),
        Span::raw(" "),
        errored,
        feedback,
        mark_span(app, thread, theme),
        Span::raw(" "),
    ])
}

fn placeholder(app: &App) -> Option<(String, bool)> {
    if let (Status::Error(message), true) = (&app.status, app.threads.is_empty()) {
        return Some((message.clone(), true));
    }
    if app.loading == Loading::First && app.threads.is_empty() {
        return Some(("loading...".to_owned(), false));
    }
    let since = &app.settings.since_label;
    match (app.first_page_loaded, app.threads.is_empty()) {
        (true, true) => Some((format!("no threads since {since}, try --since 30d"), false)),
        (true, false) if app.visible().is_empty() => Some(("no thread matches the filter".to_owned(), false)),
        _ => None,
    }
}

/// The rows, or the message that says why there are none.
pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, app: &App, theme: &Theme) {
    if let Some((message, is_error)) = placeholder(app) {
        let colour = if is_error { theme.danger } else { theme.muted };
        frame.render_widget(Paragraph::new(Line::from(Span::styled(format!("  {message}"), Style::default().fg(colour)))), area);
        return;
    }
    let height = usize::from(area.height);
    let top = (app.selected + 1).saturating_sub(height);
    let visible = app.visible();
    let rows: Vec<Line<'_>> = visible
        .iter()
        .enumerate()
        .skip(top)
        .take(height)
        .map(|(position, index)| row(app, &app.threads[*index], position == app.selected, usize::from(area.width), theme))
        .collect();
    frame.render_widget(Paragraph::new(rows), area);
}

#[cfg(test)]
mod tests {
    use super::super::tests::{app_with, now, render, settings, thread};
    use crate::app::{Action, Msg, Status, update};
    use crate::marks::Marks;

    #[test]
    fn rows_show_the_message_turns_and_the_error_and_feedback_markers() {
        let app = app_with(vec![thread("a", "How many leave days", 1, true), thread("b", "Plain question", 0, false)]);
        let screen = render(&app, 80, 8);
        assert!(screen.contains("How many leave days"), "{screen}");
        assert!(screen.contains("2h ago"));
        assert!(screen.lines().any(|line| line.contains("How many leave days") && line.contains('E') && line.contains('F')));
        assert!(screen.lines().any(|line| line.contains("Plain question") && !line.contains('E')));
        assert!(screen.contains("2 loaded") && screen.contains("more"));
    }

    #[test]
    fn a_marked_thread_shows_the_todo_marker() {
        let app = app_with(vec![thread("a", "Marked one", 0, false)]);
        let (app, _) = update(app, Msg::Action(Action::Mark));
        let (app, _) = update(app, Msg::Action(Action::Submit));
        assert!(render(&app, 80, 6).lines().any(|line| line.contains("Marked one") && line.contains('*')));
    }

    #[test]
    fn without_a_key_the_body_says_so() {
        let mut app = crate::app::App::new(settings(false), Marks::default(), now());
        app.start();
        let screen = render(&app, 80, 6);
        assert!(screen.contains("LANGSMITH_API_KEY is not set"), "{screen}");
    }

    #[test]
    fn an_empty_window_suggests_a_longer_one() {
        let screen = render(&app_with(Vec::new()), 80, 6);
        assert!(screen.contains("no threads since 7d, try --since 30d"), "{screen}");
    }

    #[test]
    fn an_error_after_rows_loaded_goes_to_the_status_line_and_keeps_the_rows() {
        let app = app_with(vec![thread("a", "Still here", 0, false)]);
        let (app, _) = update(app, Msg::Failure("network error, press r".into()));
        assert_eq!(app.status, Status::Error("network error, press r".into()));
        let screen = render(&app, 80, 6);
        assert!(screen.contains("Still here") && screen.contains("network error, press r"), "{screen}");
    }

    #[test]
    fn a_narrow_screen_cuts_the_message_instead_of_overflowing() {
        let app = app_with(vec![thread("a", &"long ".repeat(40), 0, false)]);
        assert!(render(&app, 40, 5).contains('…'));
    }
}
