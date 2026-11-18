//! Links to a trace in the LangSmith web app, filled from the configured template.

/// What a link template can name.
#[derive(Clone, Copy, Debug)]
pub struct UrlParts<'a> {
    /// The organization id.
    pub org_id: &'a str,
    /// The project uuid.
    pub project_id: &'a str,
    /// The thread id.
    pub thread_id: &'a str,
    /// The trace id.
    pub trace_id: &'a str,
}

/// `template` with `{org_id}`, `{project_id}`, `{thread_id}` and `{trace_id}` replaced.
pub fn fill(template: &str, parts: UrlParts<'_>) -> String {
    template
        .replace("{org_id}", parts.org_id)
        .replace("{project_id}", parts.project_id)
        .replace("{thread_id}", parts.thread_id)
        .replace("{trace_id}", parts.trace_id)
}

/// Whether a link may be handed to the browser opener.
pub fn is_openable(url: &str) -> bool {
    url.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_placeholder_is_filled() {
        let parts = UrlParts { org_id: "o", project_id: "p", thread_id: "th", trace_id: "tr" };
        assert_eq!(fill("https://x/{org_id}/{project_id}/{thread_id}/{trace_id}", parts), "https://x/o/p/th/tr");
    }

    #[test]
    fn only_https_links_open() {
        assert!(is_openable("https://smith.langchain.com/x"));
        assert!(!is_openable("http://smith.langchain.com/x"));
        assert!(!is_openable("file:///etc/passwd"));
    }
}
