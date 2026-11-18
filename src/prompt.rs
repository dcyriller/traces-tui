//! The prompt handed to the fix command: a template filled with the marked threads, never with message text.
use chrono::{DateTime, Utc};

/// The prompt used when the config sets none.
pub const DEFAULT_TEMPLATE: &str = "\
Fix the agent behaviour problems in these {count} conversations from the LangSmith project {project}.
Each line is a thread: read its turns through the link, or with the LangSmith tools if they are available (thread id and project above), find the cause in this repository, and fix it.
The note says what is wrong. The traces hold real employee data: do not paste their content into code, tests, commits or comments.

{threads}";

const NO_NOTE: &str = "(no note)";
const NO_LINK: &str = "(no link)";

/// One line of the prompt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkedThread {
    /// The LangSmith link, when one can be built.
    pub url: Option<String>,
    /// The thread id.
    pub thread_id: String,
    /// When it was marked.
    pub marked_at: DateTime<Utc>,
    /// What the user typed.
    pub note: String,
}

/// The filled prompt, and the placeholders the template names that nothing fills.
#[derive(Debug, PartialEq, Eq)]
pub struct Rendered {
    /// The prompt text.
    pub text: String,
    /// Unknown placeholders, left as written.
    pub unknown: Vec<String>,
}

fn one_line(note: &str) -> String {
    let joined = note.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join(" ");
    if joined.is_empty() { NO_NOTE.to_owned() } else { joined }
}

fn thread_line(thread: &MarkedThread) -> String {
    let url = thread.url.as_deref().unwrap_or(NO_LINK);
    let date = thread.marked_at.format("%Y-%m-%d");
    format!("- {url} (thread {}, marked {date}): {}", thread.thread_id, one_line(&thread.note))
}

/// Fills `{count}`, `{project}` and `{threads}` in one pass, so a note that holds a placeholder is never filled again.
pub fn render(template: &str, project: &str, threads: &[MarkedThread]) -> Rendered {
    let lines: Vec<String> = threads.iter().map(thread_line).collect();
    let mut text = String::new();
    let mut unknown = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        text.push_str(&rest[..open]);
        let after = &rest[open..];
        let Some(close) = after.find('}') else { break };
        let name = &after[1..close];
        match name {
            "count" => text.push_str(&threads.len().to_string()),
            "project" => text.push_str(project),
            "threads" => text.push_str(&lines.join("\n")),
            _ => {
                text.push_str(&after[..=close]);
                if name.chars().all(|c| c.is_ascii_lowercase() || c == '_') && !name.is_empty() {
                    unknown.push(after[..=close].to_owned());
                }
            }
        }
        rest = &after[close + 1..];
    }
    text.push_str(rest);
    Rendered { text, unknown }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(id: &str, note: &str) -> MarkedThread {
        MarkedThread {
            url: Some(format!("https://smith.example/{id}")),
            thread_id: id.to_owned(),
            marked_at: "2026-03-01T10:00:00Z".parse().unwrap(),
            note: note.to_owned(),
        }
    }

    #[test]
    fn it_renders_the_count_the_project_and_one_line_per_thread_with_link_and_note() {
        let rendered = render(DEFAULT_TEMPLATE, "app-production", &[thread("a", "wrong country"), thread("b", "too long")]);
        assert!(rendered.text.contains("these 2 conversations from the LangSmith project app-production"));
        assert!(rendered.text.contains("- https://smith.example/a (thread a, marked 2026-03-01): wrong country"));
        assert!(rendered.text.contains("- https://smith.example/b (thread b, marked 2026-03-01): too long"));
        assert_eq!(rendered.unknown, Vec::<String>::new());
    }

    #[test]
    fn a_note_with_newlines_is_joined_with_spaces() {
        let rendered = render("{threads}", "p", &[thread("a", "first\n\nsecond\r\nthird")]);
        assert!(rendered.text.ends_with(": first second third"), "{}", rendered.text);
    }

    #[test]
    fn an_empty_note_renders_as_no_note() {
        assert!(render("{threads}", "p", &[thread("a", "  \n ")]).text.ends_with("(no note)"));
    }

    #[test]
    fn an_unknown_placeholder_is_left_as_is_and_reported() {
        let rendered = render("{cuont} of {count}", "p", &[thread("a", "")]);
        assert_eq!(rendered.text, "{cuont} of 1");
        assert_eq!(rendered.unknown, ["{cuont}"]);
    }

    #[test]
    fn a_placeholder_typed_in_a_note_is_not_filled() {
        let rendered = render("{threads}", "p", &[thread("a", "see {project}")]);
        assert!(rendered.text.ends_with("see {project}"));
    }

    #[test]
    fn a_thread_without_a_link_says_so() {
        let bare = MarkedThread { url: None, ..thread("a", "n") };
        assert!(render("{threads}", "p", &[bare]).text.starts_with("- (no link) (thread a"));
    }
}
