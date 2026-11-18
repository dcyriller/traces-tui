//! The key table: one list of keys per screen, read both to translate a key press and to build the help overlay.
use crate::app::{Action, Confirm, Edit, Mode, Msg};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vtui::help::{Row, Section};

/// A group of keys that share a screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Context {
    List,
    Detail,
    Input,
    Confirm,
    Help,
}

const CONTEXTS: [(Context, &str); 5] = [
    (Context::List, "list"),
    (Context::Detail, "thread"),
    (Context::Input, "note and search"),
    (Context::Confirm, "question"),
    (Context::Help, "this help"),
];

struct Binding {
    context: Context,
    code: KeyCode,
    mods: KeyModifiers,
    action: Action,
    keys: &'static str,
    does: &'static str,
}

const NONE: KeyModifiers = KeyModifiers::NONE;
const CTRL: KeyModifiers = KeyModifiers::CONTROL;

const fn bind(context: Context, code: KeyCode, mods: KeyModifiers, action: Action, keys: &'static str, does: &'static str) -> Binding {
    Binding { context, code, mods, action, keys, does }
}

const fn ch(c: char) -> KeyCode {
    KeyCode::Char(c)
}

use Action as A;
use Context as C;

/// An empty `keys` marks an alias that the help overlay leaves out.
const BINDINGS: &[Binding] = &[
    bind(C::List, ch('j'), NONE, A::MoveDown, "j k", "move"),
    bind(C::List, KeyCode::Down, NONE, A::MoveDown, "", ""),
    bind(C::List, ch('k'), NONE, A::MoveUp, "", ""),
    bind(C::List, KeyCode::Up, NONE, A::MoveUp, "", ""),
    bind(C::List, ch('g'), NONE, A::First, "g", "first thread"),
    bind(C::List, ch('G'), NONE, A::Last, "G", "last thread, loads more"),
    bind(C::List, KeyCode::Enter, NONE, A::Open, "enter", "open the thread"),
    bind(C::List, ch('m'), NONE, A::Mark, "m", "mark or edit the note"),
    bind(C::List, ch('u'), NONE, A::Unmark, "u", "unmark"),
    bind(C::List, ch('f'), NONE, A::CycleFilter, "f", "filter: all, marked, todo"),
    bind(C::List, ch('/'), NONE, A::Search, "/", "search first messages"),
    bind(C::List, ch('r'), NONE, A::Reload, "r", "reload"),
    bind(C::List, ch('n'), NONE, A::LoadMore, "n", "load the next page"),
    bind(C::List, ch('F'), NONE, A::Fix, "F", "launch the fix command on the marks"),
    bind(C::List, ch('o'), NONE, A::OpenUrl, "o", "open in the browser"),
    bind(C::List, ch('?'), NONE, A::Help, "?", "keys"),
    bind(C::List, ch('q'), NONE, A::Quit, "q", "quit"),
    bind(C::Detail, ch('j'), NONE, A::MoveDown, "j k", "scroll"),
    bind(C::Detail, KeyCode::Down, NONE, A::MoveDown, "", ""),
    bind(C::Detail, ch('k'), NONE, A::MoveUp, "", ""),
    bind(C::Detail, KeyCode::Up, NONE, A::MoveUp, "", ""),
    bind(C::Detail, ch('d'), CTRL, A::HalfDown, "ctrl-d ctrl-u", "half a page"),
    bind(C::Detail, ch('u'), CTRL, A::HalfUp, "", ""),
    bind(C::Detail, ch('g'), NONE, A::First, "g G", "top, bottom"),
    bind(C::Detail, ch('G'), NONE, A::Last, "", ""),
    bind(C::Detail, ch('n'), NONE, A::NextTurn, "n p", "next, previous turn"),
    bind(C::Detail, ch('p'), NONE, A::PrevTurn, "", ""),
    bind(C::Detail, ch('m'), NONE, A::Mark, "m", "mark or edit the note"),
    bind(C::Detail, ch('o'), NONE, A::OpenUrl, "o", "open in the browser"),
    bind(C::Detail, KeyCode::Esc, NONE, A::Back, "esc q", "back"),
    bind(C::Detail, ch('q'), NONE, A::Back, "", ""),
    bind(C::Detail, ch('?'), NONE, A::Help, "?", "keys"),
    bind(C::Input, KeyCode::Enter, NONE, A::Submit, "enter", "save"),
    bind(C::Input, KeyCode::Esc, NONE, A::Cancel, "esc", "cancel"),
    bind(C::Confirm, ch('y'), NONE, A::Yes, "y", "yes"),
    bind(C::Confirm, ch('n'), NONE, A::No, "n esc", "no"),
    bind(C::Confirm, KeyCode::Esc, NONE, A::No, "", ""),
    bind(C::Help, ch('j'), NONE, A::MoveDown, "j k", "scroll"),
    bind(C::Help, KeyCode::Down, NONE, A::MoveDown, "", ""),
    bind(C::Help, ch('k'), NONE, A::MoveUp, "", ""),
    bind(C::Help, KeyCode::Up, NONE, A::MoveUp, "", ""),
    bind(C::Help, ch('?'), NONE, A::Help, "?", "every key"),
    bind(C::Help, KeyCode::Esc, NONE, A::Back, "esc q", "close"),
    bind(C::Help, ch('q'), NONE, A::Back, "", ""),
];

fn context_of(mode: &Mode, help_open: bool) -> Context {
    match mode {
        _ if help_open => Context::Help,
        Mode::List => Context::List,
        Mode::Detail => Context::Detail,
        Mode::Note | Mode::Search => Context::Input,
        Mode::Confirm(Confirm::Fix | Confirm::Unmark(_) | Confirm::QuitUnsaved) => Context::Confirm,
    }
}

fn edit_of(key: &KeyEvent) -> Option<Edit> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('w') if ctrl => Some(Edit::DeleteWord),
        KeyCode::Char('u') if ctrl => Some(Edit::ClearLine),
        KeyCode::Char('a') if ctrl => Some(Edit::Start),
        KeyCode::Char('e') if ctrl => Some(Edit::End),
        KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => Some(Edit::Char(c)),
        KeyCode::Backspace => Some(Edit::Backspace),
        KeyCode::Delete => Some(Edit::Delete),
        KeyCode::Left => Some(Edit::Left),
        KeyCode::Right => Some(Edit::Right),
        KeyCode::Home => Some(Edit::Start),
        KeyCode::End => Some(Edit::End),
        _ => None,
    }
}

fn is_quit(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)
}

/// The message a key press means in the current screen; none when the key does nothing there.
pub fn translate(mode: &Mode, help_open: bool, key: KeyEvent) -> Option<Msg> {
    if is_quit(&key) {
        return Some(Msg::Action(Action::Quit));
    }
    let context = context_of(mode, help_open);
    let mods = key.modifiers - KeyModifiers::SHIFT;
    let found = BINDINGS.iter().find(|binding| binding.context == context && binding.code == key.code && binding.mods == mods);
    match (found, context) {
        (Some(binding), _) => Some(Msg::Action(binding.action)),
        (None, Context::Input) => edit_of(&key).map(Msg::Edit),
        (None, _) => None,
    }
}

/// The help overlay's sections: the current screen's keys, or every screen's.
pub fn sections(mode: &Mode, every_key: bool) -> Vec<Section> {
    let current = context_of(mode, false);
    let wanted = |context: Context| every_key || context == current;
    CONTEXTS.iter().filter(|(context, _)| wanted(*context)).map(|(context, title)| Section { title, rows: rows_of(*context) }).collect()
}

fn rows_of(context: Context) -> Vec<Row> {
    let listed = BINDINGS.iter().filter(|binding| binding.context == context && !binding.keys.is_empty());
    listed.map(|binding| Row { keys: binding.keys.to_owned(), does: binding.does }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    fn action_of(mode: &Mode, key: KeyEvent) -> Option<Action> {
        match translate(mode, false, key)? {
            Msg::Action(action) => Some(action),
            _ => None,
        }
    }

    #[test]
    fn a_capital_letter_arrives_with_shift_and_still_matches() {
        assert_eq!(action_of(&Mode::List, press(ch('G'), KeyModifiers::SHIFT)), Some(Action::Last));
        assert_eq!(action_of(&Mode::List, press(ch('F'), KeyModifiers::SHIFT)), Some(Action::Fix));
    }

    #[test]
    fn the_same_key_means_different_things_per_screen() {
        assert_eq!(action_of(&Mode::List, press(ch('n'), NONE)), Some(Action::LoadMore));
        assert_eq!(action_of(&Mode::Detail, press(ch('n'), NONE)), Some(Action::NextTurn));
        assert_eq!(action_of(&Mode::Confirm(Confirm::Fix), press(ch('n'), NONE)), Some(Action::No));
    }

    #[test]
    fn q_is_typed_in_the_note_but_quits_the_list() {
        assert!(matches!(translate(&Mode::Note, false, press(ch('q'), NONE)), Some(Msg::Edit(Edit::Char('q')))));
        assert_eq!(action_of(&Mode::List, press(ch('q'), NONE)), Some(Action::Quit));
    }

    #[test]
    fn ctrl_c_quits_everywhere() {
        for mode in [Mode::List, Mode::Detail, Mode::Note, Mode::Search, Mode::Confirm(Confirm::Fix)] {
            assert_eq!(action_of(&mode, press(ch('c'), CTRL)), Some(Action::Quit));
        }
    }

    #[test]
    fn an_open_help_takes_the_keys() {
        assert!(matches!(translate(&Mode::List, true, press(ch('q'), NONE)), Some(Msg::Action(Action::Back))));
    }

    #[test]
    fn help_rows_come_from_the_table_and_every_key_lists_every_screen() {
        let here = sections(&Mode::Detail, false);
        assert_eq!(here.len(), 1);
        assert!(here[0].rows.iter().any(|row| row.keys == "n p"));
        assert_eq!(sections(&Mode::Detail, true).len(), CONTEXTS.len());
    }

    #[test]
    fn every_listed_key_in_the_help_is_bound() {
        for binding in BINDINGS.iter().filter(|binding| !binding.keys.is_empty()) {
            assert!(!binding.does.is_empty(), "{}", binding.keys);
        }
    }
}
