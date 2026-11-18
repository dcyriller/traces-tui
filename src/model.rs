//! What the screens show: threads and their turns.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A thread id: the Request id the agent sets on every turn.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThreadId(pub String);

impl ThreadId {
    /// The first eight characters, enough to tell threads apart on screen.
    pub fn short(&self) -> &str {
        self.0.get(..8).unwrap_or(&self.0)
    }
}

impl fmt::Display for ThreadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A LangSmith project's uuid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectId(pub String);

/// A feedback key on a thread, with how many points it holds and their mean.
#[derive(Clone, Debug, PartialEq)]
pub struct FeedbackKey {
    /// The feedback key name.
    pub key: String,
    /// How many points it holds.
    pub n: u32,
    /// Their mean, when the key is numeric.
    pub avg: Option<f64>,
}

/// One row of the thread list.
#[derive(Clone, Debug, PartialEq)]
pub struct Thread {
    /// The thread id.
    pub id: ThreadId,
    /// When its first turn in the window started.
    pub started_at: Option<DateTime<Utc>>,
    /// When its latest turn started.
    pub last_active_at: Option<DateTime<Utc>>,
    /// The trace id of its latest turn.
    pub last_trace_id: Option<String>,
    /// The first employee message, cut by the server.
    pub first_input: Option<String>,
    /// Turns inside the window.
    pub turn_count: u32,
    /// Turns that ended in an error.
    pub errored_turns: u32,
    /// The latest error message, cut by the server.
    pub last_error: Option<String>,
    /// Feedback keys with points on the thread.
    pub feedback: Vec<FeedbackKey>,
}

impl Thread {
    /// Whether any feedback key holds a point.
    pub fn has_feedback(&self) -> bool {
        self.feedback.iter().any(|key| key.n > 0)
    }
}

/// How a turn ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnStatus {
    /// Finished with an outcome.
    Ok,
    /// Finished with the outcome `failed`.
    Failed,
    /// The run recorded an error.
    Errored,
    /// No end time yet.
    Running,
}

/// One turn of a thread: the employee message and what the agent did with it.
#[derive(Clone, Debug, PartialEq)]
pub struct Turn {
    /// The trace id.
    pub trace_id: String,
    /// When it started.
    pub started_at: Option<DateTime<Utc>>,
    /// How long it ran, in seconds.
    pub latency_s: Option<f64>,
    /// How it ended.
    pub status: TurnStatus,
    /// The employee message.
    pub input: Option<String>,
    /// What the agent said or did.
    pub reply: Option<String>,
    /// The route the turn took.
    pub route: Option<String>,
    /// The run's error, cut by the server.
    pub error: Option<String>,
}
