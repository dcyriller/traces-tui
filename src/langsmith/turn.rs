//! Turns a LangSmith run into a `Turn`: the employee message, the reply and the route, read out of loose JSON.
use super::wire::{WireThread, WireTurn, parse_time};
use crate::model::{FeedbackKey, Thread, ThreadId, Turn, TurnStatus};
use serde_json::{Map, Value};

const REPLY_KINDS: [&str; 4] = ["answer", "clarify", "close", "reply"];
const FALLBACK_CHARS: usize = 1000;

fn text_field(value: Option<&Value>, key: &str) -> Option<String> {
    value?.get(key)?.as_str().map(str::to_owned).filter(|text| !text.is_empty())
}

fn compact_rest(outputs: &Value) -> Option<String> {
    let rest: Map<String, Value> = outputs
        .as_object()?
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "kind" | "routeTaken"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if rest.is_empty() {
        return None;
    }
    Some(vtui::text::truncate(&Value::Object(rest).to_string(), FALLBACK_CHARS))
}

fn escalation_text(outputs: &Value) -> String {
    match text_field(Some(outputs), "draftedAnswer") {
        Some(draft) => format!("handed off to HR\n{draft}"),
        None => "handed off to HR".to_owned(),
    }
}

fn reply_of(outputs: Option<&Value>) -> Option<String> {
    let outputs = outputs?;
    let kind = outputs.get("kind").and_then(Value::as_str);
    match kind {
        Some(kind) if REPLY_KINDS.contains(&kind) => text_field(Some(outputs), "content").or_else(|| compact_rest(outputs)),
        Some("escalate") => Some(escalation_text(outputs)),
        Some("confirm") => Some("(confirmation card)".to_owned()),
        Some("failed") => Some("(turn failed)".to_owned()),
        _ => compact_rest(outputs),
    }
}

fn status_of(wire: &WireTurn, error: Option<&String>) -> TurnStatus {
    let kind = wire.outputs.as_ref().and_then(|outputs| outputs.get("kind")).and_then(Value::as_str);
    if error.is_some() {
        TurnStatus::Errored
    } else if kind == Some("failed") {
        TurnStatus::Failed
    } else if wire.end_time.is_none() {
        TurnStatus::Running
    } else {
        TurnStatus::Ok
    }
}

/// Reads one run of a thread.
pub(crate) fn to_turn(wire: WireTurn) -> Turn {
    let error = wire.error_preview.clone().filter(|text| !text.is_empty());
    Turn {
        status: status_of(&wire, error.as_ref()),
        input: text_field(wire.inputs.as_ref(), "input"),
        reply: reply_of(wire.outputs.as_ref()),
        route: text_field(wire.outputs.as_ref(), "routeTaken"),
        started_at: wire.start_time.as_deref().and_then(parse_time),
        latency_s: wire.latency,
        trace_id: wire.trace_id,
        error,
    }
}

/// The employee message out of a cut `first_inputs` preview: the `input` of its JSON when it still parses, else the raw text.
pub(crate) fn preview_text(preview: &str) -> Option<String> {
    let parsed = serde_json::from_str::<Value>(preview).ok();
    let text = text_field(parsed.as_ref(), "input").unwrap_or_else(|| preview.to_owned());
    Some(text).filter(|text| !text.trim().is_empty())
}

/// Reads one row of the thread list.
pub(crate) fn to_thread(wire: WireThread) -> Thread {
    let mut feedback: Vec<FeedbackKey> = wire
        .feedback_stats
        .unwrap_or_default()
        .into_iter()
        .map(|(key, stat)| FeedbackKey { key, n: stat.n.unwrap_or(0), avg: stat.avg })
        .collect();
    feedback.sort_by(|a, b| a.key.cmp(&b.key));
    Thread {
        id: ThreadId(wire.thread_id),
        started_at: wire.min_start_time.as_deref().and_then(parse_time),
        last_active_at: wire.max_start_time.as_deref().and_then(parse_time),
        last_trace_id: wire.last_trace_id,
        first_input: wire.first_inputs.as_deref().and_then(preview_text),
        turn_count: wire.count.unwrap_or(0),
        errored_turns: wire.num_errored_turns.unwrap_or(0),
        last_error: wire.last_error.filter(|text| !text.is_empty()),
        feedback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn wire(inputs: Value, outputs: Value) -> WireTurn {
        WireTurn {
            trace_id: "t1".into(),
            start_time: Some("2026-03-01T10:00:00Z".into()),
            end_time: Some("2026-03-01T10:00:02Z".into()),
            latency: Some(2.0),
            inputs: Some(inputs),
            outputs: Some(outputs),
            error_preview: None,
        }
    }

    #[test]
    fn answer_clarify_close_and_reply_read_content() {
        for kind in ["answer", "clarify", "close", "reply"] {
            let turn =
                to_turn(wire(json!({"input": "How many days?"}), json!({"kind": kind, "content": "Twenty-five.", "routeTaken": "search"})));
            assert_eq!(turn.reply.as_deref(), Some("Twenty-five."), "{kind}");
            assert_eq!(turn.input.as_deref(), Some("How many days?"));
            assert_eq!(turn.route.as_deref(), Some("search"));
            assert_eq!(turn.status, TurnStatus::Ok);
        }
    }

    #[test]
    fn escalate_hides_the_note_to_hr_and_shows_the_draft() {
        let turn =
            to_turn(wire(json!({"input": "q"}), json!({"kind": "escalate", "opsContent": "NOTE TO HR", "draftedAnswer": "Maybe 5 days."})));
        let reply = turn.reply.unwrap();
        assert!(reply.contains("handed off to HR") && reply.contains("Maybe 5 days."));
        assert!(!reply.contains("NOTE TO HR"));
        let bare = to_turn(wire(json!({"input": "q"}), json!({"kind": "escalate", "opsContent": "NOTE", "draftedAnswer": null})));
        assert_eq!(bare.reply.as_deref(), Some("handed off to HR"));
    }

    #[test]
    fn confirm_shows_a_placeholder() {
        let turn = to_turn(wire(json!({"input": "q"}), json!({"kind": "confirm", "card": {"a": 1}, "submission": {}})));
        assert_eq!(turn.reply.as_deref(), Some("(confirmation card)"));
    }

    #[test]
    fn a_failed_outcome_marks_the_turn_failed() {
        let turn = to_turn(wire(json!({"input": "q"}), json!({"kind": "failed"})));
        assert_eq!(turn.status, TurnStatus::Failed);
    }

    #[test]
    fn a_run_error_marks_the_turn_errored_and_keeps_the_text() {
        let mut run = wire(json!({"input": "q"}), json!({}));
        run.error_preview = Some("timeout".into());
        let turn = to_turn(run);
        assert_eq!((turn.status, turn.error.as_deref()), (TurnStatus::Errored, Some("timeout")));
    }

    #[test]
    fn no_end_time_means_running() {
        let mut run = wire(json!({"input": "q"}), json!({}));
        run.end_time = None;
        assert_eq!(to_turn(run).status, TurnStatus::Running);
    }

    #[test]
    fn a_missing_input_is_none() {
        let turn = to_turn(wire(json!({"other": 1}), json!({"kind": "answer", "content": "x"})));
        assert_eq!(turn.input, None);
    }

    #[test]
    fn an_unknown_shape_falls_back_to_compact_json_without_kind_and_route() {
        let turn = to_turn(wire(json!({"input": "q"}), json!({"kind": "mystery", "routeTaken": "search", "extra": [1, 2]})));
        assert_eq!(turn.reply.as_deref(), Some(r#"{"extra":[1,2]}"#));
        let empty = to_turn(wire(json!({"input": "q"}), json!({"kind": "mystery"})));
        assert_eq!(empty.reply, None);
    }

    #[test]
    fn a_preview_reads_the_input_of_its_json_else_stays_raw() {
        assert_eq!(preview_text(r#"{"input": "Hello there"}"#).as_deref(), Some("Hello there"));
        assert_eq!(preview_text(r#"{"input": "Hello th"#).as_deref(), Some(r#"{"input": "Hello th"#));
        assert_eq!(preview_text("  "), None);
    }

    #[test]
    fn a_thread_row_reads_counts_feedback_and_previews() {
        let row = WireThread {
            thread_id: "0123456789abcdef".into(),
            min_start_time: Some("2026-03-01T10:00:00".into()),
            max_start_time: Some("2026-03-02T10:00:00".into()),
            count: Some(3),
            first_inputs: Some(r#"{"input":"Hi"}"#.into()),
            num_errored_turns: Some(1),
            last_error: Some(String::new()),
            feedback_stats: Some([("score".to_owned(), crate::langsmith::wire::WireFeedback { n: Some(2), avg: Some(0.5) })].into()),
            last_trace_id: Some("tr".into()),
        };
        let thread = to_thread(row);
        assert_eq!((thread.turn_count, thread.errored_turns, thread.last_error.as_deref()), (3, 1, None));
        assert_eq!(thread.first_input.as_deref(), Some("Hi"));
        assert!(thread.has_feedback());
        assert_eq!(thread.id.short(), "01234567");
    }
}
