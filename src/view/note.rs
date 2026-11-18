//! The popups: the note input and the y/n question.
use crate::app::{App, Confirm, Mode};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use vtui::field::Field;
use vtui::pane::framed;
use vtui::theme::Theme;

const POPUP_WIDTH: u16 = 64;
const POPUP_HEIGHT: u16 = 3;

/// The input text with the character under the cursor reversed.
pub(super) fn field_line(field: &Field) -> Line<'static> {
    let (before, under, after) = field.split();
    let cursor = if under.is_empty() { " " } else { under };
    Line::from(vec![
        Span::raw(before.to_owned()),
        Span::styled(cursor.to_owned(), Style::default().add_modifier(Modifier::REVERSED)),
        Span::raw(after.to_owned()),
    ])
}

fn popup(frame: &mut Frame<'_>, area: Rect, title: &str, line: Line<'static>, theme: &Theme) {
    let width = POPUP_WIDTH.min(area.width);
    let height = POPUP_HEIGHT.min(area.height);
    let rect = Rect { x: area.x + (area.width - width) / 2, y: area.y + (area.height - height) / 2, width, height };
    frame.render_widget(Clear, rect);
    frame.render_widget(Paragraph::new(line).block(framed(theme, title, true)), rect);
}

fn question(app: &App, confirm: &Confirm) -> String {
    match confirm {
        Confirm::Unmark(thread) => format!("Remove the mark on thread {}? y/n", thread.short()),
        Confirm::Fix => {
            let count = app.marks.todo(&app.settings.project).len();
            format!("Launch {} on {count} threads in {}? y/n", app.settings.fix_program, app.settings.repo_dir)
        }
        Confirm::QuitUnsaved => "Quit and drop the note you typed? y/n".to_owned(),
    }
}

/// Draws the popup the mode asks for, over the screen.
pub(super) fn draw(frame: &mut Frame<'_>, app: &App, theme: &Theme) {
    let area = frame.area();
    match &app.mode {
        Mode::Note => popup(frame, area, "note, enter saves, esc cancels", field_line(&app.field), theme),
        Mode::Confirm(confirm) => popup(frame, area, "confirm", Line::raw(question(app, confirm)), theme),
        Mode::List | Mode::Detail | Mode::Search => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{app_with, render, thread};
    use crate::app::{Action, Edit, Msg, update};

    #[test]
    fn the_note_popup_shows_what_is_typed() {
        let (app, _) = update(app_with(vec![thread("a", "q", 0, false)]), Msg::Action(Action::Mark));
        let (app, _) = update(app, Msg::Edit(Edit::Char('h')));
        let (app, _) = update(app, Msg::Edit(Edit::Char('i')));
        let screen = render(&app, 80, 12);
        assert!(screen.contains("note, enter saves") && screen.contains("hi"), "{screen}");
    }

    #[test]
    fn the_fix_question_names_the_program_the_count_and_the_repo() {
        let app = app_with(vec![thread("a", "q", 0, false)]);
        let (app, _) = update(app, Msg::Action(Action::Mark));
        let (app, _) = update(app, Msg::Action(Action::Submit));
        let (app, _) = update(app, Msg::Action(Action::Fix));
        assert!(render(&app, 90, 12).contains("Launch claude on 1 threads in /repo? y/n"));
    }

    #[test]
    fn no_popup_on_the_list() {
        assert!(!render(&app_with(vec![thread("a", "q", 0, false)]), 80, 8).contains("confirm"));
    }
}
