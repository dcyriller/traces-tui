//! One thread's turns, as lines the app scrolls through.
use crate::app::{App, Detail, Fit};
use crate::model::{Turn, TurnStatus};
use chrono::Local;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vtui::text::wrap_line;
use vtui::theme::Theme;

fn status_span(turn: &Turn, theme: &Theme) -> Span<'static> {
    match turn.status {
        TurnStatus::Ok => Span::styled("ok", Style::default().fg(theme.success)),
        TurnStatus::Failed => Span::styled("failed", Style::default().fg(theme.danger)),
        TurnStatus::Errored => Span::styled("errored", Style::default().fg(theme.danger)),
        TurnStatus::Running => Span::styled("running", Style::default().fg(theme.warn)),
    }
}

fn header_line(turn: &Turn, is_current: bool, theme: &Theme) -> Line<'static> {
    let time = turn.started_at.map_or_else(|| "-".to_owned(), |at| at.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string());
    let route = turn.route.clone().unwrap_or_else(|| "-".to_owned());
    let latency = turn.latency_s.map_or_else(String::new, |seconds| format!("  {seconds:.1}s"));
    let weight = if is_current { Modifier::BOLD } else { Modifier::empty() };
    let style = Style::default().fg(theme.accent).add_modifier(weight);
    Line::from(vec![
        Span::styled(format!("{time}  {route}  "), style),
        status_span(turn, theme),
        Span::styled(latency, Style::default().fg(theme.muted)),
    ])
}

fn body_lines(label: &str, text: Option<&str>, colour: ratatui::style::Color, theme: &Theme, width: usize) -> Vec<Line<'static>> {
    let text = text.unwrap_or("(none)");
    let label_style = Style::default().fg(theme.muted);
    let rows = text.lines().enumerate().map(|(index, row)| {
        let head = if index == 0 { format!("{label:<10}") } else { " ".repeat(10) };
        Line::from(vec![Span::styled(head, label_style), Span::styled(row.to_owned(), Style::default().fg(colour))])
    });
    rows.flat_map(|line| wrap_line(&line, width)).collect()
}

fn turn_lines(turn: &Turn, is_current: bool, theme: &Theme, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![header_line(turn, is_current, theme)];
    lines.extend(body_lines("employee:", turn.input.as_deref(), theme.muted, theme, width));
    lines.extend(body_lines("agent:", turn.reply.as_deref(), theme.success, theme, width));
    if let Some(error) = &turn.error {
        lines.extend(body_lines("error:", Some(error), theme.danger, theme, width));
    }
    lines.push(Line::raw(""));
    lines
}

fn feedback_label(key: &crate::model::FeedbackKey) -> String {
    let avg = key.avg.map_or_else(String::new, |avg| format!(" {avg:.2}"));
    format!("{} {}{avg}", key.key, key.n)
}

fn title_line(detail: &Detail, theme: &Theme) -> Line<'static> {
    let truncated = if detail.truncated { "  truncated" } else { "" };
    let feedback: Vec<String> = detail.thread.feedback.iter().filter(|key| key.n > 0).map(feedback_label).collect();
    let text = format!(" thread {}  {} turns{truncated}  {}", detail.thread.id.short(), detail.turns.len(), feedback.join("  "));
    Line::from(Span::styled(text, Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)))
}

/// Draws the open thread and says how far it scrolls and where each turn starts.
pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, app: &App, theme: &Theme) -> Fit {
    let Some(detail) = &app.detail else { return Fit::default() };
    let [title_area, body] =
        ratatui::layout::Layout::vertical([ratatui::layout::Constraint::Length(1), ratatui::layout::Constraint::Min(1)]).areas(area);
    frame.render_widget(Paragraph::new(title_line(detail, theme)), title_area);
    if detail.loading || detail.error.is_some() {
        let (message, colour) = detail.error.as_ref().map_or(("loading...", theme.muted), |error| (error.as_str(), theme.danger));
        frame.render_widget(Paragraph::new(Line::from(Span::styled(format!("  {message}"), Style::default().fg(colour)))), body);
        return Fit::default();
    }
    let width = usize::from(body.width).saturating_sub(2);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut turn_starts = Vec::new();
    for (index, turn) in detail.turns.iter().enumerate() {
        turn_starts.push(lines.len());
        lines.extend(turn_lines(turn, index == detail.turn, theme, width));
    }
    let page = usize::from(body.height);
    let shown: Vec<Line<'static>> = lines.iter().skip(detail.scroll).take(page).cloned().collect();
    let padded = Paragraph::new(shown).block(ratatui::widgets::Block::default().padding(ratatui::widgets::Padding::horizontal(1)));
    frame.render_widget(padded, body);
    Fit { max_scroll: lines.len().saturating_sub(page), turn_starts, page }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{app_with, render, thread};
    use crate::app::{Action, Fit, Msg, update};
    use crate::langsmith::TurnsPage;
    use crate::model::{Turn, TurnStatus};

    fn turn(input: &str, reply: &str, status: TurnStatus) -> Turn {
        Turn {
            trace_id: "t".into(),
            started_at: Some("2026-03-01T10:00:00Z".parse().unwrap()),
            latency_s: Some(2.5),
            status,
            input: Some(input.into()),
            reply: Some(reply.into()),
            route: Some("search".into()),
            error: (status == TurnStatus::Errored).then(|| "boom".to_owned()),
        }
    }

    fn opened(turns: Vec<Turn>) -> crate::app::App {
        let (app, _) = update(app_with(vec![thread("abcdef1234", "q", 0, true)]), Msg::Action(Action::Open));
        update(app, Msg::Turns { generation: 1, result: Ok(TurnsPage { turns, truncated: false }) }).0
    }

    #[test]
    fn it_shows_each_turn_with_route_status_input_reply_and_error() {
        let app =
            opened(vec![turn("How many days?", "Twenty-five.", TurnStatus::Ok), turn("Again?", "(turn failed)", TurnStatus::Errored)]);
        let screen = render(&app, 70, 16);
        for expected in ["thread abcdef12", "2 turns", "score 1", "search", "ok", "How many days?", "Twenty-five.", "errored", "boom"] {
            assert!(screen.contains(expected), "{expected} in\n{screen}");
        }
    }

    #[test]
    fn it_says_loading_and_shows_a_failed_load() {
        let (app, _) = update(app_with(vec![thread("a", "q", 0, false)]), Msg::Action(Action::Open));
        assert!(render(&app, 60, 6).contains("loading..."));
        let (app, _) = update(app, Msg::Turns { generation: 1, result: Err("network error, press r".into()) });
        assert!(render(&app, 60, 6).contains("network error, press r"));
    }

    #[test]
    fn a_long_reply_wraps_and_the_fit_reports_where_each_turn_starts() {
        let app = opened(vec![turn("q", &"word ".repeat(60), TurnStatus::Ok), turn("second", "r", TurnStatus::Ok)]);
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 10)).unwrap();
        let mut fit = Fit::default();
        terminal.draw(|frame| fit = super::super::draw(frame, &app, &vtui::theme::Theme::default())).unwrap();
        assert_eq!(fit.turn_starts[0], 0);
        assert!(fit.turn_starts[1] > 5, "{fit:?}");
        assert!(fit.max_scroll > 0);
    }
}
