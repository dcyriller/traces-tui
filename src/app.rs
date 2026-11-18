//! The pure state of the TUI: `update` takes a message and returns the next state and the effects to run.
use crate::keys::sections;
use crate::langsmith::url::{UrlParts, fill, is_openable};
use crate::langsmith::{ThreadsPage, TurnsPage};
use crate::marks::{Mark, MarkStatus, Marks};
use crate::model::{ProjectId, Thread, ThreadId, Turn};
use chrono::{DateTime, Utc};
use vtui::field::Field;
use vtui::help::{Help, last_row};

const LOAD_MORE_WITHIN_ROWS: usize = 5;

/// A key's meaning in the current screen, translated by `keys`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Down one row or line.
    MoveDown,
    /// Up one row or line.
    MoveUp,
    /// To the first row or line.
    First,
    /// To the last row or line.
    Last,
    /// Half a page down.
    HalfDown,
    /// Half a page up.
    HalfUp,
    /// To the next turn.
    NextTurn,
    /// To the previous turn.
    PrevTurn,
    /// Open the thread.
    Open,
    /// Mark the thread or edit its note.
    Mark,
    /// Remove the mark.
    Unmark,
    /// Cycle the list filter.
    CycleFilter,
    /// Search the first messages.
    Search,
    /// Reload from the first page.
    Reload,
    /// Launch the fix command.
    Fix,
    /// Open the thread in the browser.
    OpenUrl,
    /// Load the next page of threads.
    LoadMore,
    /// Leave the screen or close the overlay.
    Back,
    /// Quit.
    Quit,
    /// Show or toggle the key help.
    Help,
    /// Confirm the typed text.
    Submit,
    /// Drop the typed text.
    Cancel,
    /// Answer yes.
    Yes,
    /// Answer no.
    No,
}

/// An edit of the one-line input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Type a character.
    Char(char),
    /// Erase before the cursor.
    Backspace,
    /// Erase after the cursor.
    Delete,
    /// Erase the word before the cursor.
    DeleteWord,
    /// Erase back to the line start.
    ClearLine,
    /// Cursor left.
    Left,
    /// Cursor right.
    Right,
    /// Cursor to the start.
    Start,
    /// Cursor to the end.
    End,
}

/// What a fix launch ended with.
#[derive(Debug)]
pub struct FixDone {
    /// The threads handed to the command.
    pub threads: Vec<ThreadId>,
    /// `Err` holds what to show.
    pub outcome: Result<(), String>,
}

/// Everything the update function reacts to.
#[derive(Debug)]
pub enum Msg {
    /// A key with a meaning on this screen.
    Action(Action),
    /// A key that edits the input.
    Edit(Edit),
    /// Pasted text.
    Paste(String),
    /// The spinner moves on.
    Tick,
    /// The project name resolved.
    Project {
        /// The list generation the request belongs to.
        generation: u64,
        /// The uuid, or what to show.
        result: Result<ProjectId, String>,
    },
    /// A page of threads arrived.
    Threads {
        /// The list generation the request belongs to.
        generation: u64,
        /// The page, or what to show.
        result: Result<ThreadsPage, String>,
    },
    /// The turns of the open thread arrived.
    Turns {
        /// The turns generation the request belongs to.
        generation: u64,
        /// The turns, or what to show.
        result: Result<TurnsPage, String>,
    },
    /// The fix command ended.
    Fix(FixDone),
    /// Something outside the state failed, with what to show.
    Failure(String),
}

/// What the event loop does for the state.
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    /// Resolve the project name.
    ResolveProject {
        /// The list generation to answer with.
        generation: u64,
    },
    /// Fetch a page of threads.
    LoadThreads {
        /// The list generation to answer with.
        generation: u64,
        /// The project.
        project: ProjectId,
        /// The page cursor; none for the first page.
        cursor: Option<String>,
    },
    /// Fetch the turns of a thread.
    LoadTurns {
        /// The turns generation to answer with.
        generation: u64,
        /// The project.
        project: ProjectId,
        /// The thread.
        thread: ThreadId,
    },
    /// Write the marks file.
    SaveMarks,
    /// Launch the fix command on these threads.
    Fix(Vec<ThreadId>),
    /// Open a link in the browser.
    OpenUrl(String),
    /// Leave.
    Quit,
}

/// What the list shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    /// Every thread.
    All,
    /// Threads with a mark.
    Marked,
    /// Threads with a mark still to fix.
    Todo,
}

/// A question waiting for y or n.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Confirm {
    /// Remove this mark.
    Unmark(ThreadId),
    /// Launch the fix command.
    Fix,
    /// Quit with a note not saved.
    QuitUnsaved,
}

/// Which screen takes the keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The thread list.
    List,
    /// One thread's turns.
    Detail,
    /// The note input.
    Note,
    /// The search input.
    Search,
    /// A y/n question.
    Confirm(Confirm),
}

/// What the status line says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Nothing to say.
    Idle,
    /// A neutral message.
    Info(String),
    /// A failure.
    Error(String),
}

/// What is being fetched for the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loading {
    /// Nothing.
    Idle,
    /// The first page.
    First,
    /// A later page.
    More,
}

/// What the detail screen measured while drawing, so scrolling stays inside the text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fit {
    /// The largest scroll offset.
    pub max_scroll: usize,
    /// The line each turn starts on.
    pub turn_starts: Vec<usize>,
    /// Lines visible at once.
    pub page: usize,
}

/// The open thread.
#[derive(Clone, Debug)]
pub struct Detail {
    /// The thread opened.
    pub thread: Thread,
    /// Its turns, oldest first.
    pub turns: Vec<Turn>,
    /// Whether more turns exist than were read.
    pub truncated: bool,
    /// Whether the turns are on their way.
    pub loading: bool,
    /// Why they did not arrive.
    pub error: Option<String>,
    /// The turn `n` and `p` stand on.
    pub turn: usize,
    /// The first visible line.
    pub scroll: usize,
    /// What the last draw measured.
    pub fit: Fit,
}

/// What the app knows about the run from the start.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The project name.
    pub project: String,
    /// The window of the list, as typed.
    pub since_label: String,
    /// The program the fix command runs.
    pub fix_program: String,
    /// The repository it runs in.
    pub repo_dir: String,
    /// The link template.
    pub url_template: String,
    /// The organization id for links.
    pub org_id: String,
    /// Whether `LANGSMITH_API_KEY` is set.
    pub has_key: bool,
}

/// The whole state.
#[derive(Debug)]
pub struct App {
    /// The run's settings.
    pub settings: Settings,
    /// The resolved project.
    pub project_id: Option<ProjectId>,
    /// The loaded threads, newest activity first.
    pub threads: Vec<Thread>,
    /// The cursor of the next page.
    pub next_cursor: Option<String>,
    /// Whether the first page arrived.
    pub first_page_loaded: bool,
    /// What is being fetched.
    pub loading: Loading,
    /// The marks.
    pub marks: Marks,
    /// The selected row of the visible list.
    pub selected: usize,
    /// The list filter.
    pub filter: Filter,
    /// The search text.
    pub search: String,
    /// The screen taking the keys.
    pub mode: Mode,
    /// The note or search input.
    pub field: Field,
    /// The open thread.
    pub detail: Option<Detail>,
    /// The status line.
    pub status: Status,
    /// The help overlay, when open.
    pub help: Option<Help>,
    /// Clock reading set by the loop before each update.
    pub now: DateTime<Utc>,
    /// Frame counter of the spinner.
    pub spinner: usize,
    list_generation: u64,
    turns_generation: u64,
    note_target: Option<Thread>,
    note_initial: String,
    quit: bool,
}

/// Applies `msg` and returns the next state with the effects to run.
pub fn update(mut app: App, msg: Msg) -> (App, Vec<Effect>) {
    let effects = app.apply(msg);
    (app, effects)
}

impl App {
    /// A fresh state on the list.
    pub fn new(settings: Settings, marks: Marks, now: DateTime<Utc>) -> Self {
        Self {
            settings,
            project_id: None,
            threads: Vec::new(),
            next_cursor: None,
            first_page_loaded: false,
            loading: Loading::Idle,
            marks,
            selected: 0,
            filter: Filter::All,
            search: String::new(),
            mode: Mode::List,
            field: Field::default(),
            detail: None,
            status: Status::Idle,
            help: None,
            now,
            spinner: 0,
            list_generation: 0,
            turns_generation: 0,
            note_target: None,
            note_initial: String::new(),
            quit: false,
        }
    }

    /// The effects that fill the list at start.
    pub fn start(&mut self) -> Vec<Effect> {
        self.reload()
    }

    /// Whether a fetch is in flight, so the spinner ticks.
    pub fn is_busy(&self) -> bool {
        self.loading != Loading::Idle || self.detail.as_ref().is_some_and(|detail| detail.loading)
    }

    /// Whether the user asked to leave.
    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// Takes what the last draw measured for the detail screen.
    pub fn fit(&mut self, fit: Fit) {
        if let Some(detail) = &mut self.detail {
            detail.scroll = detail.scroll.min(fit.max_scroll);
            detail.fit = fit;
        }
    }

    /// Indexes into `threads` of the rows the filter and the search let through.
    pub fn visible(&self) -> Vec<usize> {
        let needle = self.search.to_lowercase();
        let keeps = |thread: &Thread| self.passes_filter(thread) && matches_search(thread, &needle);
        self.threads.iter().enumerate().filter(|(_, thread)| keeps(thread)).map(|(index, _)| index).collect()
    }

    /// The mark of a thread.
    pub fn mark_of(&self, thread: &Thread) -> Option<&Mark> {
        self.marks.get(&self.settings.project, &thread.id)
    }

    /// The thread the selection stands on.
    pub fn selected_thread(&self) -> Option<&Thread> {
        self.visible().get(self.selected).and_then(|index| self.threads.get(*index))
    }

    /// The link of a thread's latest turn.
    pub fn url_of(&self, thread: &Thread) -> Option<String> {
        let project_id = self.project_id.as_ref()?;
        let trace_id = thread.last_trace_id.as_deref()?;
        let parts = UrlParts { org_id: &self.settings.org_id, project_id: &project_id.0, thread_id: &thread.id.0, trace_id };
        Some(fill(&self.settings.url_template, parts))
    }

    fn passes_filter(&self, thread: &Thread) -> bool {
        match (self.filter, self.mark_of(thread)) {
            (Filter::All, _) => true,
            (Filter::Marked, mark) => mark.is_some(),
            (Filter::Todo, mark) => mark.is_some_and(|mark| mark.status == MarkStatus::Todo),
        }
    }

    fn apply(&mut self, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Action(action) => self.act(action),
            Msg::Edit(edit) => self.edit(edit),
            Msg::Paste(text) => self.paste(&text),
            Msg::Tick => {
                self.spinner = self.spinner.wrapping_add(1);
                Vec::new()
            }
            Msg::Project { generation, result } => self.on_project(generation, result),
            Msg::Threads { generation, result } => self.on_threads(generation, result),
            Msg::Turns { generation, result } => self.on_turns(generation, result),
            Msg::Fix(done) => self.on_fix(done),
            Msg::Failure(message) => {
                self.status = Status::Error(message);
                Vec::new()
            }
        }
    }

    fn act(&mut self, action: Action) -> Vec<Effect> {
        if self.help.is_some() {
            return self.act_help(action);
        }
        match self.mode.clone() {
            Mode::List => self.act_list(action),
            Mode::Detail => self.act_detail(action),
            Mode::Note | Mode::Search => self.act_input(action),
            Mode::Confirm(question) => self.act_confirm(&question, action),
        }
    }

    fn act_help(&mut self, action: Action) -> Vec<Effect> {
        let Some(help) = &mut self.help else { return Vec::new() };
        match action {
            Action::Back => self.help = None,
            Action::Help => help.every_key = !help.every_key,
            Action::MoveDown => help.scroll = (help.scroll + 1).min(last_row(&sections(&self.mode, help.every_key))),
            Action::MoveUp => help.scroll = help.scroll.saturating_sub(1),
            Action::Quit => return self.quit(),
            _ => {}
        }
        Vec::new()
    }

    fn act_list(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::MoveDown => self.select(self.selected.saturating_add(1)),
            Action::MoveUp => self.select(self.selected.saturating_sub(1)),
            Action::First => self.select(0),
            Action::Last => self.select(usize::MAX),
            Action::Open => self.open_detail(),
            Action::Mark => self.selected_thread().cloned().map_or_else(Vec::new, |thread| self.open_note(thread)),
            Action::Unmark => self.ask_unmark(),
            Action::CycleFilter => self.cycle_filter(),
            Action::Search => self.open_search(),
            Action::Reload => self.reload(),
            Action::Fix => self.ask_fix(),
            Action::OpenUrl => self.open_url(self.selected_thread().cloned()),
            Action::LoadMore => self.load_more(),
            Action::Help => self.show_help(),
            Action::Quit => self.quit(),
            _ => Vec::new(),
        }
    }

    fn act_detail(&mut self, action: Action) -> Vec<Effect> {
        let thread = self.detail.as_ref().map(|detail| detail.thread.clone());
        match (action, thread) {
            (Action::Mark, Some(thread)) => return self.open_note(thread),
            (Action::OpenUrl, thread) => return self.open_url(thread),
            (Action::Back, _) => return self.close_detail(),
            (Action::Help, _) => return self.show_help(),
            (Action::Quit, _) => return self.quit(),
            _ => {}
        }
        let Some(detail) = &mut self.detail else { return Vec::new() };
        let half = (detail.fit.page / 2).max(1);
        match action {
            Action::MoveDown => detail.scroll = detail.scroll.saturating_add(1).min(detail.fit.max_scroll),
            Action::MoveUp => detail.scroll = detail.scroll.saturating_sub(1),
            Action::HalfDown => detail.scroll = detail.scroll.saturating_add(half).min(detail.fit.max_scroll),
            Action::HalfUp => detail.scroll = detail.scroll.saturating_sub(half),
            Action::First => detail.scroll = 0,
            Action::Last => detail.scroll = detail.fit.max_scroll,
            Action::NextTurn => step_turn(detail, 1),
            Action::PrevTurn => step_turn(detail, -1),
            _ => {}
        }
        Vec::new()
    }

    fn act_input(&mut self, action: Action) -> Vec<Effect> {
        let is_note = self.mode == Mode::Note;
        match (action, is_note) {
            (Action::Submit, true) => self.save_note(),
            (Action::Submit, false) => self.leave_input(),
            (Action::Cancel, true) => self.leave_input(),
            (Action::Cancel, false) => {
                self.search.clear();
                self.leave_input()
            }
            (Action::Quit, true) if self.field.text() != self.note_initial => {
                self.mode = Mode::Confirm(Confirm::QuitUnsaved);
                Vec::new()
            }
            (Action::Quit, _) => self.quit(),
            _ => Vec::new(),
        }
    }

    fn act_confirm(&mut self, question: &Confirm, action: Action) -> Vec<Effect> {
        match (action, question) {
            (Action::Yes, Confirm::Unmark(thread)) => {
                self.marks.remove(&self.settings.project, thread);
                self.mode = self.back_mode();
                self.clamp_selection();
                vec![Effect::SaveMarks]
            }
            (Action::Yes, Confirm::Fix) => {
                self.mode = self.back_mode();
                self.status = Status::Info(format!("launching {}", self.settings.fix_program));
                vec![Effect::Fix(self.todo_threads())]
            }
            (Action::Yes, Confirm::QuitUnsaved) => self.quit(),
            (Action::No, Confirm::QuitUnsaved) => {
                self.mode = Mode::Note;
                Vec::new()
            }
            (Action::No | Action::Back, _) => {
                self.mode = self.back_mode();
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn back_mode(&self) -> Mode {
        if self.detail.is_some() { Mode::Detail } else { Mode::List }
    }

    fn quit(&mut self) -> Vec<Effect> {
        self.quit = true;
        vec![Effect::Quit]
    }

    fn show_help(&mut self) -> Vec<Effect> {
        self.help = Some(Help::default());
        Vec::new()
    }

    fn select(&mut self, row: usize) -> Vec<Effect> {
        let last = self.visible().len().saturating_sub(1);
        self.selected = row.min(last);
        let near_end = self.selected + LOAD_MORE_WITHIN_ROWS >= self.visible().len();
        if near_end { self.load_more() } else { Vec::new() }
    }

    fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.visible().len().saturating_sub(1));
    }

    fn load_more(&mut self) -> Vec<Effect> {
        let (Some(cursor), Some(project)) = (self.next_cursor.clone(), self.project_id.clone()) else { return Vec::new() };
        if self.loading != Loading::Idle {
            return Vec::new();
        }
        self.loading = Loading::More;
        vec![Effect::LoadThreads { generation: self.list_generation, project, cursor: Some(cursor) }]
    }

    fn reload(&mut self) -> Vec<Effect> {
        self.list_generation += 1;
        self.next_cursor = None;
        if !self.settings.has_key {
            self.status = Status::Error("LANGSMITH_API_KEY is not set".to_owned());
            self.loading = Loading::Idle;
            return Vec::new();
        }
        self.status = Status::Idle;
        self.loading = Loading::First;
        let generation = self.list_generation;
        match self.project_id.clone() {
            Some(project) => vec![Effect::LoadThreads { generation, project, cursor: None }],
            None => vec![Effect::ResolveProject { generation }],
        }
    }

    fn cycle_filter(&mut self) -> Vec<Effect> {
        self.filter = match self.filter {
            Filter::All => Filter::Marked,
            Filter::Marked => Filter::Todo,
            Filter::Todo => Filter::All,
        };
        self.selected = 0;
        Vec::new()
    }

    fn open_search(&mut self) -> Vec<Effect> {
        self.field = Field::new(self.search.clone());
        self.mode = Mode::Search;
        Vec::new()
    }

    fn leave_input(&mut self) -> Vec<Effect> {
        self.mode = self.back_mode();
        self.clamp_selection();
        Vec::new()
    }

    fn open_detail(&mut self) -> Vec<Effect> {
        let (Some(thread), Some(project)) = (self.selected_thread().cloned(), self.project_id.clone()) else { return Vec::new() };
        self.turns_generation += 1;
        let effect = Effect::LoadTurns { generation: self.turns_generation, project, thread: thread.id.clone() };
        self.detail = Some(Detail {
            thread,
            turns: Vec::new(),
            truncated: false,
            loading: true,
            error: None,
            turn: 0,
            scroll: 0,
            fit: Fit::default(),
        });
        self.mode = Mode::Detail;
        vec![effect]
    }

    fn close_detail(&mut self) -> Vec<Effect> {
        self.turns_generation += 1;
        self.detail = None;
        self.mode = Mode::List;
        Vec::new()
    }

    fn open_note(&mut self, thread: Thread) -> Vec<Effect> {
        let initial = self.mark_of(&thread).map(|mark| mark.note.clone()).unwrap_or_default();
        self.field = Field::new(initial.clone());
        self.note_initial = initial;
        self.note_target = Some(thread);
        self.mode = Mode::Note;
        Vec::new()
    }

    fn save_note(&mut self) -> Vec<Effect> {
        let Some(thread) = self.note_target.take() else { return self.leave_input() };
        let mark = Mark {
            thread_id: thread.id,
            last_trace_id: thread.last_trace_id,
            project: self.settings.project.clone(),
            project_id: self.project_id.as_ref().map(|id| id.0.clone()),
            note: self.field.take(),
            marked_at: self.now,
            status: MarkStatus::Todo,
            sent_at: None,
        };
        self.marks.upsert(mark);
        self.mode = self.back_mode();
        vec![Effect::SaveMarks]
    }

    fn ask_unmark(&mut self) -> Vec<Effect> {
        match self.selected_thread().filter(|thread| self.mark_of(thread).is_some()).map(|thread| thread.id.clone()) {
            Some(id) => self.mode = Mode::Confirm(Confirm::Unmark(id)),
            None => self.status = Status::Info("not marked".to_owned()),
        }
        Vec::new()
    }

    fn todo_threads(&self) -> Vec<ThreadId> {
        self.marks.todo(&self.settings.project).iter().map(|mark| mark.thread_id.clone()).collect()
    }

    fn ask_fix(&mut self) -> Vec<Effect> {
        if self.todo_threads().is_empty() {
            self.status = Status::Info("no marked threads to fix".to_owned());
        } else {
            self.mode = Mode::Confirm(Confirm::Fix);
        }
        Vec::new()
    }

    fn open_url(&mut self, thread: Option<Thread>) -> Vec<Effect> {
        let url = thread.and_then(|thread| self.url_of(&thread));
        match url {
            Some(url) if is_openable(&url) => vec![Effect::OpenUrl(url)],
            Some(_) => {
                self.status = Status::Error("only https links open".to_owned());
                Vec::new()
            }
            None => {
                self.status = Status::Error("no link for this thread yet".to_owned());
                Vec::new()
            }
        }
    }

    fn edit(&mut self, edit: Edit) -> Vec<Effect> {
        if !matches!(self.mode, Mode::Note | Mode::Search) {
            return Vec::new();
        }
        match edit {
            Edit::Char(c) => self.field.insert(c),
            Edit::Backspace => self.field.backspace(),
            Edit::Delete => self.field.delete(),
            Edit::DeleteWord => self.field.delete_word(),
            Edit::ClearLine => self.field.delete_to_line_start(),
            Edit::Left => self.field.left(),
            Edit::Right => self.field.right(),
            Edit::Start => self.field.start(),
            Edit::End => self.field.end(),
        }
        self.follow_search();
        Vec::new()
    }

    fn paste(&mut self, text: &str) -> Vec<Effect> {
        if matches!(self.mode, Mode::Note | Mode::Search) {
            self.field.paste(text);
            self.follow_search();
        }
        Vec::new()
    }

    fn follow_search(&mut self) {
        if self.mode == Mode::Search {
            self.search = self.field.text().to_owned();
            self.selected = 0;
        }
    }

    fn on_project(&mut self, generation: u64, result: Result<ProjectId, String>) -> Vec<Effect> {
        if generation != self.list_generation {
            return Vec::new();
        }
        match result {
            Ok(project) => {
                self.project_id = Some(project.clone());
                vec![Effect::LoadThreads { generation, project, cursor: None }]
            }
            Err(message) => self.fail_list(message),
        }
    }

    fn fail_list(&mut self, message: String) -> Vec<Effect> {
        self.loading = Loading::Idle;
        self.status = Status::Error(message);
        Vec::new()
    }

    fn on_threads(&mut self, generation: u64, result: Result<ThreadsPage, String>) -> Vec<Effect> {
        if generation != self.list_generation {
            return Vec::new();
        }
        let Ok(page) = result else { return self.fail_list(result.err().unwrap_or_default()) };
        let appending = self.loading == Loading::More;
        self.loading = Loading::Idle;
        self.status = Status::Idle;
        self.first_page_loaded = true;
        self.next_cursor = page.next_cursor;
        if !appending {
            self.threads.clear();
        }
        let known: Vec<ThreadId> = self.threads.iter().map(|thread| thread.id.clone()).collect();
        self.threads.extend(page.threads.into_iter().filter(|thread| !known.contains(&thread.id)));
        self.threads.sort_by_key(|thread| std::cmp::Reverse(thread.last_active_at));
        self.clamp_selection();
        Vec::new()
    }

    fn on_turns(&mut self, generation: u64, result: Result<TurnsPage, String>) -> Vec<Effect> {
        let Some(detail) = self.detail.as_mut().filter(|_| generation == self.turns_generation) else { return Vec::new() };
        detail.loading = false;
        match result {
            Ok(page) => {
                detail.turns = page.turns;
                detail.truncated = page.truncated;
            }
            Err(message) => detail.error = Some(message),
        }
        Vec::new()
    }

    fn on_fix(&mut self, done: FixDone) -> Vec<Effect> {
        match done.outcome {
            Ok(()) => {
                self.marks.mark_sent(&self.settings.project, &done.threads, self.now);
                self.status = Status::Info(format!("{} threads sent", done.threads.len()));
                vec![Effect::SaveMarks]
            }
            Err(message) => {
                self.status = Status::Error(message);
                Vec::new()
            }
        }
    }
}

fn matches_search(thread: &Thread, needle: &str) -> bool {
    needle.is_empty() || thread.first_input.as_deref().is_some_and(|text| text.to_lowercase().contains(needle))
}

fn step_turn(detail: &mut Detail, by: isize) {
    let last = detail.turns.len().saturating_sub(1);
    detail.turn = detail.turn.saturating_add_signed(by).min(last);
    if let Some(start) = detail.fit.turn_starts.get(detail.turn) {
        detail.scroll = (*start).min(detail.fit.max_scroll);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(has_key: bool) -> Settings {
        Settings {
            project: "app-production".into(),
            since_label: "7d".into(),
            fix_program: "claude".into(),
            repo_dir: "/repo".into(),
            url_template: "https://smith.example/{project_id}/{trace_id}".into(),
            org_id: "org".into(),
            has_key,
        }
    }

    fn thread(id: &str, input: &str, age_hours: i64) -> Thread {
        let at = "2026-03-02T00:00:00Z".parse::<DateTime<Utc>>().unwrap() - chrono::Duration::hours(age_hours);
        Thread {
            id: ThreadId(id.into()),
            started_at: Some(at),
            last_active_at: Some(at),
            last_trace_id: Some(format!("tr-{id}")),
            first_input: Some(input.into()),
            turn_count: 1,
            errored_turns: 0,
            last_error: None,
            feedback: Vec::new(),
        }
    }

    fn page(threads: Vec<Thread>, cursor: Option<&str>) -> ThreadsPage {
        ThreadsPage { threads, next_cursor: cursor.map(str::to_owned) }
    }

    fn loaded(threads: Vec<Thread>, cursor: Option<&str>) -> App {
        let now = "2026-03-02T00:00:00Z".parse().unwrap();
        let mut app = App::new(settings(true), Marks::default(), now);
        app.start();
        let (app, _) = update(app, Msg::Project { generation: 1, result: Ok(ProjectId("p".into())) });
        let (app, _) = update(app, Msg::Threads { generation: 1, result: Ok(page(threads, cursor)) });
        app
    }

    fn press(app: App, action: Action) -> (App, Vec<Effect>) {
        update(app, Msg::Action(action))
    }

    fn type_text(mut app: App, text: &str) -> App {
        for c in text.chars() {
            app = update(app, Msg::Edit(Edit::Char(c))).0;
        }
        app
    }

    #[test]
    fn without_a_key_the_list_says_so_and_starts_no_fetch() {
        let mut app = App::new(settings(false), Marks::default(), Utc::now());
        assert_eq!(app.start(), Vec::new());
        assert_eq!(app.status, Status::Error("LANGSMITH_API_KEY is not set".into()));
        let (app, _) = press(app, Action::Mark);
        assert_eq!(app.mode, Mode::List);
    }

    #[test]
    fn start_resolves_the_project_then_loads_the_first_page() {
        let mut app = App::new(settings(true), Marks::default(), Utc::now());
        assert_eq!(app.start(), vec![Effect::ResolveProject { generation: 1 }]);
        let (_, effects) = update(app, Msg::Project { generation: 1, result: Ok(ProjectId("p".into())) });
        assert_eq!(effects, vec![Effect::LoadThreads { generation: 1, project: ProjectId("p".into()), cursor: None }]);
    }

    #[test]
    fn an_error_keeps_the_loaded_rows() {
        let app = loaded(vec![thread("a", "hi", 1)], Some("c"));
        let (app, _) = press(app, Action::Reload);
        let (app, _) = update(app, Msg::Threads { generation: 2, result: Err("network error, press r".into()) });
        assert_eq!(app.threads.len(), 1);
        assert_eq!(app.status, Status::Error("network error, press r".into()));
        assert_eq!(app.loading, Loading::Idle);
    }

    #[test]
    fn a_stale_response_is_dropped() {
        let app = loaded(vec![thread("a", "hi", 1)], None);
        let (app, _) = press(app, Action::Reload);
        let (app, _) = update(app, Msg::Threads { generation: 1, result: Ok(page(vec![thread("old", "x", 9)], None)) });
        assert_eq!(app.threads[0].id.0, "a");
        assert_eq!(app.loading, Loading::First);
    }

    #[test]
    fn the_first_page_replaces_and_a_later_page_appends_without_duplicates() {
        let app = loaded(vec![thread("a", "hi", 2)], Some("c1"));
        let (app, effects) = press(app, Action::LoadMore);
        assert_eq!(effects, vec![Effect::LoadThreads { generation: 1, project: ProjectId("p".into()), cursor: Some("c1".into()) }]);
        let (app, _) =
            update(app, Msg::Threads { generation: 1, result: Ok(page(vec![thread("a", "dup", 2), thread("b", "new", 1)], None)) });
        let ids: Vec<&str> = app.threads.iter().map(|thread| thread.id.0.as_str()).collect();
        assert_eq!(ids, ["b", "a"]);
        assert_eq!(app.next_cursor, None);
    }

    #[test]
    fn moving_near_the_end_asks_for_the_next_page_once() {
        let threads: Vec<Thread> = (0..6).map(|i| thread(&format!("t{i}"), "x", i)).collect();
        let app = loaded(threads, Some("c1"));
        let (app, effects) = press(app, Action::MoveDown);
        assert_eq!(effects.len(), 1);
        let (_, again) = press(app, Action::MoveDown);
        assert_eq!(again, Vec::new());
    }

    #[test]
    fn the_selection_never_leaves_the_visible_rows() {
        let app = loaded(vec![thread("a", "x", 1), thread("b", "y", 2)], None);
        let (app, _) = press(app, Action::Last);
        assert_eq!(app.selected, 1);
        let (app, _) = press(app, Action::MoveDown);
        assert_eq!(app.selected, 1);
        let (app, _) = press(app, Action::First);
        let (app, _) = press(app, Action::MoveUp);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn marking_saves_a_todo_mark_with_the_typed_note_and_editing_reuses_it() {
        let app = loaded(vec![thread("a", "x", 1)], None);
        let (app, _) = press(app, Action::Mark);
        assert_eq!(app.mode, Mode::Note);
        let app = type_text(app, "wrong country");
        let (app, effects) = press(app, Action::Submit);
        assert_eq!((app.mode.clone(), effects), (Mode::List, vec![Effect::SaveMarks]));
        let mark = app.marks.get("app-production", &ThreadId("a".into())).unwrap();
        assert_eq!((mark.note.as_str(), mark.status, mark.project_id.as_deref()), ("wrong country", MarkStatus::Todo, Some("p")));
        let (app, _) = press(app, Action::Mark);
        assert_eq!(app.field.text(), "wrong country");
    }

    #[test]
    fn cancelling_a_note_saves_nothing() {
        let app = loaded(vec![thread("a", "x", 1)], None);
        let (app, _) = press(app, Action::Mark);
        let app = type_text(app, "n");
        let (app, effects) = press(app, Action::Cancel);
        assert_eq!((app.marks.all().len(), effects), (0, Vec::new()));
    }

    #[test]
    fn unmark_asks_first_and_says_so_when_there_is_no_mark() {
        let app = loaded(vec![thread("a", "x", 1)], None);
        let (app, _) = press(app, Action::Unmark);
        assert_eq!(app.status, Status::Info("not marked".into()));
        let (app, _) = press(app, Action::Mark);
        let (app, _) = press(app, Action::Submit);
        let (app, _) = press(app, Action::Unmark);
        assert!(matches!(app.mode, Mode::Confirm(Confirm::Unmark(_))));
        let (app, effects) = press(app, Action::Yes);
        assert_eq!((app.marks.all().len(), effects), (0, vec![Effect::SaveMarks]));
    }

    #[test]
    fn the_filter_cycles_through_all_marked_and_todo() {
        let app = loaded(vec![thread("a", "x", 1), thread("b", "y", 2)], None);
        let (app, _) = press(app, Action::Mark);
        let (app, _) = press(app, Action::Submit);
        let (app, _) = press(app, Action::CycleFilter);
        assert_eq!((app.filter, app.visible().len()), (Filter::Marked, 1));
        let (app, _) = press(app, Action::CycleFilter);
        assert_eq!(app.filter, Filter::Todo);
        let (app, _) = press(app, Action::CycleFilter);
        assert_eq!((app.filter, app.visible().len()), (Filter::All, 2));
    }

    #[test]
    fn search_filters_the_first_messages_ignoring_case_and_escape_clears_it() {
        let app = loaded(vec![thread("a", "Parental LEAVE", 1), thread("b", "salary", 2)], None);
        let (app, _) = press(app, Action::Search);
        let app = type_text(app, "leave");
        assert_eq!(app.visible(), vec![0]);
        let (app, _) = press(app, Action::Cancel);
        assert_eq!((app.visible().len(), &app.mode), (2, &Mode::List));
    }

    #[test]
    fn quitting_asks_only_when_the_note_holds_unsaved_text() {
        let app = loaded(vec![thread("a", "x", 1)], None);
        let (app, _) = press(app, Action::Mark);
        let (clean, _) = press(app, Action::Quit);
        assert!(clean.should_quit());
        let app = loaded(vec![thread("a", "x", 1)], None);
        let (app, _) = press(app, Action::Mark);
        let app = type_text(app, "draft");
        let (app, effects) = press(app, Action::Quit);
        assert_eq!((app.should_quit(), effects, app.mode.clone()), (false, Vec::new(), Mode::Confirm(Confirm::QuitUnsaved)));
        let (back, _) = press(app, Action::No);
        assert_eq!(back.mode, Mode::Note);
    }

    #[test]
    fn opening_a_thread_loads_its_turns_and_a_stale_answer_after_closing_is_dropped() {
        let app = loaded(vec![thread("a", "x", 1)], None);
        let (app, effects) = press(app, Action::Open);
        assert!(matches!(effects.as_slice(), [Effect::LoadTurns { generation: 1, .. }]));
        let (app, _) = press(app, Action::Back);
        let (app, _) = update(app, Msg::Turns { generation: 1, result: Ok(TurnsPage { turns: Vec::new(), truncated: false }) });
        assert_eq!((app.detail.is_none(), &app.mode), (true, &Mode::List));
    }

    #[test]
    fn fix_asks_then_launches_and_a_success_flips_the_marks_to_sent() {
        let app = loaded(vec![thread("a", "x", 1)], None);
        let (app, _) = press(app, Action::Fix);
        assert_eq!(app.status, Status::Info("no marked threads to fix".into()));
        let (app, _) = press(app, Action::Mark);
        let (app, _) = press(app, Action::Submit);
        let (app, _) = press(app, Action::Fix);
        assert_eq!(app.mode, Mode::Confirm(Confirm::Fix));
        let (app, effects) = press(app, Action::Yes);
        assert_eq!(effects, vec![Effect::Fix(vec![ThreadId("a".into())])]);
        let done = FixDone { threads: vec![ThreadId("a".into())], outcome: Ok(()) };
        let (app, effects) = update(app, Msg::Fix(done));
        assert_eq!(effects, vec![Effect::SaveMarks]);
        assert_eq!(app.marks.todo("app-production").len(), 0);
    }

    #[test]
    fn a_failed_fix_keeps_the_marks_todo_and_shows_the_exit() {
        let app = loaded(vec![thread("a", "x", 1)], None);
        let (app, _) = press(app, Action::Mark);
        let (app, _) = press(app, Action::Submit);
        let done = FixDone { threads: vec![ThreadId("a".into())], outcome: Err("claude exited with exit status: 1".into()) };
        let (app, _) = update(app, Msg::Fix(done));
        assert_eq!(app.marks.todo("app-production").len(), 1);
        assert!(matches!(app.status, Status::Error(_)));
    }

    #[test]
    fn open_url_builds_the_link_from_the_latest_trace_and_refuses_a_non_https_template() {
        let app = loaded(vec![thread("a", "x", 1)], None);
        let (mut app, effects) = press(app, Action::OpenUrl);
        assert_eq!(effects, vec![Effect::OpenUrl("https://smith.example/p/tr-a".into())]);
        app.settings.url_template = "http://smith.example/{trace_id}".into();
        let (app, effects) = press(app, Action::OpenUrl);
        assert_eq!((effects, &app.status), (Vec::new(), &Status::Error("only https links open".into())));
    }

    #[test]
    fn next_turn_scrolls_to_the_turn_start_and_stays_inside_the_text() {
        let app = loaded(vec![thread("a", "x", 1)], None);
        let (mut app, _) = press(app, Action::Open);
        if let Some(detail) = app.detail.as_mut() {
            detail.turns = vec![
                Turn {
                    trace_id: "1".into(),
                    started_at: None,
                    latency_s: None,
                    status: crate::model::TurnStatus::Ok,
                    input: None,
                    reply: None,
                    route: None,
                    error: None
                };
                2
            ];
        }
        app.fit(Fit { max_scroll: 7, turn_starts: vec![0, 9], page: 10 });
        let (app, _) = press(app, Action::NextTurn);
        assert_eq!(app.detail.as_ref().map(|detail| (detail.turn, detail.scroll)), Some((1, 7)));
    }
}
