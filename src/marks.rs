//! The marks store: threads to fix, kept in one JSON file. It holds ids and the note you typed, never message text.
use crate::model::{ProjectId, ThreadId};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

const VERSION: u32 = 1;

/// Whether a mark went to the fix command yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum MarkStatus {
    /// Waiting for a fix session.
    Todo,
    /// Handed to a fix command that exited 0; says nothing about the fix landing.
    Sent,
}

/// A thread marked to fix, with the note saying what is wrong.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    /// The marked thread.
    pub thread_id: ThreadId,
    /// The trace id of its latest turn when marked, for the link.
    pub last_trace_id: Option<String>,
    /// The project it belongs to.
    pub project: String,
    /// The project's uuid when marked, for the link.
    pub project_id: Option<ProjectId>,
    /// What the user typed.
    pub note: String,
    /// When it was marked.
    pub marked_at: DateTime<Utc>,
    /// Whether it was sent.
    pub status: MarkStatus,
    /// When it was sent.
    pub sent_at: Option<DateTime<Utc>>,
}

impl Mark {
    /// Whether this is the mark of `thread` in `project`.
    pub fn is_of(&self, project: &str, thread: &ThreadId) -> bool {
        self.project == project && self.thread_id == *thread
    }
}

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    marks: Vec<Mark>,
}

/// Every mark, one per project and thread.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    marks: Vec<Mark>,
}

impl Marks {
    /// Reads the store; a missing file is an empty store, a corrupt one an error that leaves it untouched.
    pub fn load(path: &Path) -> Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
        };
        let file: File = serde_json::from_str(&text).with_context(|| format!("{} is not a marks file", path.display()))?;
        if file.version != VERSION {
            bail!("{} has version {}, this build reads version {VERSION}", path.display(), file.version);
        }
        Ok(Self { marks: file.marks })
    }

    /// Writes the store through a temporary file and a rename, so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let dir = path.parent().context("the marks path has no folder")?;
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        let file = File { version: VERSION, marks: self.marks.clone() };
        let mut temp = tempfile::NamedTempFile::new_in(dir)?;
        temp.write_all(serde_json::to_string_pretty(&file)?.as_bytes())?;
        temp.persist(path).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// Every mark, in the order they were first made.
    pub fn all(&self) -> &[Mark] {
        &self.marks
    }

    /// The mark of a thread in a project.
    pub fn get(&self, project: &str, thread: &ThreadId) -> Option<&Mark> {
        self.marks.iter().find(|mark| mark.is_of(project, thread))
    }

    /// The marks of `project`, only those in `status` when given.
    pub fn in_project(&self, project: &str, status: Option<MarkStatus>) -> Vec<&Mark> {
        self.marks.iter().filter(|mark| mark.project == project && status.is_none_or(|wanted| mark.status == wanted)).collect()
    }

    /// The marks of `project` still waiting for a fix session.
    pub fn todo(&self, project: &str) -> Vec<&Mark> {
        self.in_project(project, Some(MarkStatus::Todo))
    }

    /// Adds `mark`, or replaces the one of the same project and thread.
    pub fn upsert(&mut self, mark: Mark) {
        match self.marks.iter_mut().find(|held| held.is_of(&mark.project, &mark.thread_id)) {
            Some(held) => *held = mark,
            None => self.marks.push(mark),
        }
    }

    /// Drops a mark; says whether there was one.
    pub fn remove(&mut self, project: &str, thread: &ThreadId) -> bool {
        let before = self.marks.len();
        self.marks.retain(|mark| !mark.is_of(project, thread));
        self.marks.len() != before
    }

    /// Flips the marks of `threads` in `project` to sent.
    pub fn mark_sent(&mut self, project: &str, threads: &[ThreadId], at: DateTime<Utc>) {
        for mark in self.marks.iter_mut().filter(|mark| mark.project == project && threads.contains(&mark.thread_id)) {
            mark.status = MarkStatus::Sent;
            mark.sent_at = Some(at);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn mark(thread: &str, note: &str) -> Mark {
        Mark {
            thread_id: ThreadId(thread.to_owned()),
            last_trace_id: Some(format!("trace-{thread}")),
            project: "app-production".to_owned(),
            project_id: Some(ProjectId("proj-1".to_owned())),
            note: note.to_owned(),
            marked_at: "2026-03-01T10:00:00Z".parse().unwrap(),
            status: MarkStatus::Todo,
            sent_at: None,
        }
    }

    #[test]
    fn upsert_replaces_the_note_for_the_same_project_and_thread() {
        let mut marks = Marks::default();
        marks.upsert(mark("a", "first"));
        marks.upsert(mark("a", "second"));
        marks.upsert(Mark { project: "staging".into(), ..mark("a", "other project") });
        assert_eq!(marks.all().len(), 2);
        assert_eq!(marks.get("app-production", &ThreadId("a".into())).unwrap().note, "second");
    }

    #[test]
    fn in_project_filters_by_project_and_status() {
        let mut marks = Marks::default();
        marks.upsert(mark("a", ""));
        marks.upsert(mark("b", ""));
        marks.upsert(Mark { project: "staging".into(), ..mark("c", "") });
        marks.mark_sent("app-production", &[ThreadId("b".into())], "2026-03-02T00:00:00Z".parse().unwrap());
        assert_eq!(marks.in_project("app-production", None).len(), 2);
        assert_eq!(marks.in_project("app-production", Some(MarkStatus::Sent))[0].thread_id.0, "b");
        assert_eq!(marks.in_project("staging", Some(MarkStatus::Sent)).len(), 0);
    }

    #[test]
    fn is_of_matches_the_project_and_the_thread() {
        let held = mark("a", "");
        assert!(held.is_of("app-production", &ThreadId("a".into())));
        assert!(!held.is_of("staging", &ThreadId("a".into())));
        assert!(!held.is_of("app-production", &ThreadId("b".into())));
    }

    #[test]
    fn remove_says_whether_a_mark_was_there() {
        let mut marks = Marks::default();
        marks.upsert(mark("a", ""));
        assert!(marks.remove("app-production", &ThreadId("a".into())));
        assert!(!marks.remove("app-production", &ThreadId("a".into())));
    }

    #[test]
    fn mark_sent_flips_only_the_listed_threads_and_leaves_todo_without_them() {
        let mut marks = Marks::default();
        marks.upsert(mark("a", ""));
        marks.upsert(mark("b", ""));
        let at: DateTime<Utc> = "2026-03-02T00:00:00Z".parse().unwrap();
        marks.mark_sent("app-production", &[ThreadId("a".into())], at);
        assert_eq!(marks.get("app-production", &ThreadId("a".into())).unwrap().sent_at, Some(at));
        let todo: Vec<&str> = marks.todo("app-production").iter().map(|mark| mark.thread_id.0.as_str()).collect();
        assert_eq!(todo, ["b"]);
    }

    #[test]
    fn save_round_trips_through_a_temp_folder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/marks.json");
        let mut marks = Marks::default();
        marks.upsert(mark("a", "wrong \"quote\"\nand a newline"));
        marks.save(&path).unwrap();
        assert_eq!(Marks::load(&path).unwrap(), marks);
    }

    #[test]
    fn a_missing_file_is_an_empty_store() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Marks::load(&dir.path().join("none.json")).unwrap(), Marks::default());
    }

    #[test]
    fn a_corrupt_file_is_an_error_naming_the_path_and_stays_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("marks.json");
        std::fs::write(&path, "{ not json").unwrap();
        let err = format!("{:#}", Marks::load(&path).unwrap_err());
        assert!(err.contains(path.to_str().unwrap()), "{err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
    }

    #[test]
    fn a_version_mismatch_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("marks.json");
        std::fs::write(&path, r#"{"version": 2, "marks": []}"#).unwrap();
        assert!(format!("{:#}", Marks::load(&path).unwrap_err()).contains("version 2"));
    }
}
