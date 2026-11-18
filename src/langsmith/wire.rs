//! The fields read from the LangSmith responses, and nothing else.
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub(crate) struct WireThread {
    pub thread_id: String,
    pub min_start_time: Option<String>,
    pub max_start_time: Option<String>,
    pub count: Option<u32>,
    pub first_inputs: Option<String>,
    pub num_errored_turns: Option<u32>,
    pub last_error: Option<String>,
    pub feedback_stats: Option<HashMap<String, WireFeedback>>,
    pub last_trace_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireFeedback {
    pub n: Option<u32>,
    pub avg: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireTurn {
    pub trace_id: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub latency: Option<f64>,
    pub inputs: Option<Value>,
    pub outputs: Option<Value>,
    pub error_preview: Option<String>,
}

/// LangSmith writes UTC times with or without an offset.
pub(crate) fn parse_time(text: &str) -> Option<DateTime<Utc>> {
    if let Ok(time) = DateTime::parse_from_rfc3339(text) {
        return Some(time.with_timezone(&Utc));
    }
    NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f").ok().map(|naive| naive.and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_parse_with_and_without_an_offset() {
        let with = parse_time("2026-03-01T10:00:00.5Z").unwrap();
        let without = parse_time("2026-03-01T10:00:00.5").unwrap();
        assert_eq!(with, without);
        assert!(parse_time("yesterday").is_none());
    }
}
