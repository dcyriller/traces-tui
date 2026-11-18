//! The LangSmith REST client: project lookup, thread list, thread turns and run links.
pub mod url;
pub(crate) mod wire;

use crate::model::{ProjectId, Thread, ThreadId, Turn};
use crate::turn::{to_thread, to_turn};
use chrono::{DateTime, SecondsFormat, Utc};
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use reqwest::{Response, StatusCode, Url};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::fmt;
use std::time::Duration;
use wire::{WireThread, WireTurn};

const REQUEST_TIMEOUT_SECONDS: u64 = 20;
const THREAD_PAGE_SIZE: u32 = 50;
const TURN_PAGE_SIZE: &str = "100";
const MAX_TURN_PAGES: usize = 5;
const SESSIONS_LIMIT: &str = "100";
const TURN_FIELDS: [&str; 8] = ["START_TIME", "END_TIME", "LATENCY", "INPUTS", "OUTPUTS", "ERROR_PREVIEW", "TRACE_ID", "TURN_NUMBER"];

/// What went wrong talking to LangSmith, in the words the status line shows.
#[derive(Debug, PartialEq, Eq)]
pub enum ApiError {
    /// 401 or 403.
    KeyRejected,
    /// No project carries the requested name.
    ProjectNotFound(String),
    /// Timeout, DNS or a refused connection.
    Network,
    /// 429.
    RateLimited,
    /// 404 or 501 on a threads endpoint.
    Unavailable(u16),
    /// Any other failing status.
    Status(u16),
    /// A body that does not read as the expected JSON.
    Decode,
    /// An `api_url` that would send the key in clear.
    InsecureUrl(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeyRejected => f.write_str("key rejected by LangSmith"),
            Self::ProjectNotFound(name) => write!(f, "project {name} not found"),
            Self::Network => f.write_str("network error"),
            Self::RateLimited => f.write_str("LangSmith is rate limiting requests (429)"),
            Self::Unavailable(code) => write!(f, "threads API unavailable ({code})"),
            Self::Status(code) => write!(f, "LangSmith answered {code}"),
            Self::Decode => f.write_str("unexpected response from LangSmith"),
            Self::InsecureUrl(url) => write!(f, "api_url {url} must be https, or http on localhost"),
        }
    }
}

impl std::error::Error for ApiError {}

/// One page of the thread list.
#[derive(Debug)]
pub struct ThreadsPage {
    /// Threads, newest activity first.
    pub threads: Vec<Thread>,
    /// The cursor of the next page, when there is one.
    pub next_cursor: Option<String>,
}

/// The turns of a thread, oldest first.
#[derive(Debug)]
pub struct TurnsPage {
    /// The turns read.
    pub turns: Vec<Turn>,
    /// Whether more turns exist than the page cap reads.
    pub truncated: bool,
}

#[derive(Deserialize)]
struct Session {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct Page<T> {
    #[serde(default = "Vec::new")]
    items: Vec<T>,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
struct RunUrl {
    url: String,
}

/// A client for one LangSmith API base and key.
pub struct Client {
    http: reqwest::Client,
    base: Url,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client").field("base", &self.base.as_str()).finish_non_exhaustive()
    }
}

fn is_safe_base(url: &Url) -> bool {
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1"));
    url.scheme() == "https" || (url.scheme() == "http" && local)
}

fn error_of(status: StatusCode) -> ApiError {
    match status.as_u16() {
        401 | 403 => ApiError::KeyRejected,
        429 => ApiError::RateLimited,
        code @ (404 | 501) => ApiError::Unavailable(code),
        code => ApiError::Status(code),
    }
}

async fn read<T: DeserializeOwned>(sent: reqwest::Result<Response>) -> Result<T, ApiError> {
    let response = sent.map_err(|_error| ApiError::Network)?;
    if !response.status().is_success() {
        return Err(error_of(response.status()));
    }
    response.json().await.map_err(|_error| ApiError::Decode)
}

impl Client {
    /// A client for `api_url` that sends `api_key` on every request.
    pub fn new(api_url: &str, api_key: &str) -> Result<Self, ApiError> {
        let base = Url::parse(api_url)
            .ok()
            .filter(|url| is_safe_base(url) && url.username().is_empty())
            .ok_or_else(|| ApiError::InsecureUrl(api_url.to_owned()))?;
        let mut key = HeaderValue::from_str(api_key).map_err(|_error| ApiError::KeyRejected)?;
        key.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", key);
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
            .build()
            .map_err(|_error| ApiError::Network)?;
        Ok(Self { http, base })
    }

    fn endpoint(&self, segments: &[&str]) -> Url {
        let mut url = self.base.clone();
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty().extend(segments);
        }
        url
    }

    /// The uuid of the project called exactly `name`.
    pub async fn resolve_project(&self, name: &str) -> Result<ProjectId, ApiError> {
        let sent =
            self.http.get(self.endpoint(&["api", "v1", "sessions"])).query(&[("name", name), ("limit", SESSIONS_LIMIT)]).send().await;
        let sessions: Vec<Session> = read(sent).await?;
        let found = sessions.into_iter().find(|session| session.name == name);
        found.map(|session| ProjectId(session.id)).ok_or_else(|| ApiError::ProjectNotFound(name.to_owned()))
    }

    /// One page of threads active since `since`, after `cursor` when given.
    pub async fn list_threads(&self, project: &ProjectId, since: DateTime<Utc>, cursor: Option<&str>) -> Result<ThreadsPage, ApiError> {
        let mut body = json!({
            "project_id": project.0,
            "page_size": THREAD_PAGE_SIZE,
            "min_start_time": since.to_rfc3339_opts(SecondsFormat::Secs, true),
        });
        if let (Some(cursor), Some(fields)) = (cursor, body.as_object_mut()) {
            fields.insert("cursor".to_owned(), Value::String(cursor.to_owned()));
        }
        let sent = self.http.post(self.endpoint(&["api", "v2", "threads", "query"])).json(&body).send().await;
        let page: Page<WireThread> = read(sent).await?;
        let mut threads: Vec<Thread> = page.items.into_iter().map(to_thread).collect();
        threads.sort_by_key(|thread| std::cmp::Reverse(thread.last_active_at));
        Ok(ThreadsPage { threads, next_cursor: page.next_cursor })
    }

    /// The turns of `thread`, oldest first, up to five pages of a hundred.
    pub async fn list_turns(&self, project: &ProjectId, thread: &ThreadId) -> Result<TurnsPage, ApiError> {
        let mut turns = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_TURN_PAGES {
            let mut query = vec![("project_id", project.0.as_str()), ("page_size", TURN_PAGE_SIZE)];
            query.extend(TURN_FIELDS.iter().map(|field| ("selects", *field)));
            if let Some(cursor) = &cursor {
                query.push(("cursor", cursor));
            }
            let sent = self.http.get(self.endpoint(&["api", "v2", "threads", &thread.0, "traces"])).query(&query).send().await;
            let page: Page<WireTurn> = read(sent).await?;
            turns.extend(page.items.into_iter().map(to_turn));
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        turns.sort_by_key(|turn| turn.started_at);
        Ok(TurnsPage { turns, truncated: cursor.is_some() })
    }

    /// The link LangSmith itself gives for a trace.
    pub async fn read_run_url(&self, project: &ProjectId, trace_id: &str) -> Result<String, ApiError> {
        let sent = self
            .http
            .get(self.endpoint(&["api", "v2", "runs", trace_id, "url"]))
            .query(&[("project_id", project.0.as_str()), ("trace_id", trace_id)])
            .send()
            .await;
        let run: RunUrl = read(sent).await?;
        Ok(run.url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const PROJECT: &str = "11111111-1111-1111-1111-111111111111";

    fn client(server: &MockServer) -> Client {
        Client::new(&server.uri(), "test-key").unwrap()
    }

    fn project() -> ProjectId {
        ProjectId(PROJECT.to_owned())
    }

    fn since() -> DateTime<Utc> {
        "2026-03-01T00:00:00Z".parse().unwrap()
    }

    #[tokio::test]
    async fn resolve_project_picks_the_exact_name_among_substring_matches() {
        let server = MockServer::start().await;
        let sessions = json!([{"id": "a", "name": "app-production-eu"}, {"id": PROJECT, "name": "app-production"}]);
        Mock::given(method("GET"))
            .and(path("/api/v1/sessions"))
            .and(query_param("name", "app-production"))
            .respond_with(ResponseTemplate::new(200).set_body_json(sessions))
            .mount(&server)
            .await;
        assert_eq!(client(&server).resolve_project("app-production").await.unwrap(), project());
    }

    #[tokio::test]
    async fn resolve_project_errors_when_no_name_matches() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/sessions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id": "a", "name": "other"}])))
            .mount(&server)
            .await;
        let err = client(&server).resolve_project("app-production").await.unwrap_err();
        assert_eq!(err, ApiError::ProjectNotFound("app-production".into()));
    }

    #[tokio::test]
    async fn list_threads_sends_the_key_the_project_the_page_size_and_an_explicit_start() {
        let server = MockServer::start().await;
        let expected = json!({"project_id": PROJECT, "page_size": 50, "min_start_time": "2026-03-01T00:00:00Z"});
        let page = json!({"items": [{"thread_id": "old", "max_start_time": "2026-03-01T01:00:00Z"}, {"thread_id": "new", "max_start_time": "2026-03-02T01:00:00Z"}], "next_cursor": null});
        Mock::given(method("POST"))
            .and(path("/api/v2/threads/query"))
            .and(header("x-api-key", "test-key"))
            .and(body_partial_json(expected))
            .respond_with(ResponseTemplate::new(200).set_body_json(page))
            .expect(1)
            .mount(&server)
            .await;
        let page = client(&server).list_threads(&project(), since(), None).await.unwrap();
        let ids: Vec<&str> = page.threads.iter().map(|thread| thread.id.0.as_str()).collect();
        assert_eq!(ids, ["new", "old"]);
        assert_eq!(page.next_cursor, None);
    }

    #[tokio::test]
    async fn list_threads_returns_the_cursor_and_a_follow_up_sends_it() {
        let server = MockServer::start().await;
        let first = json!({"items": [{"thread_id": "a"}], "next_cursor": "c1"});
        Mock::given(method("POST"))
            .and(body_partial_json(json!({"project_id": PROJECT})))
            .respond_with(ResponseTemplate::new(200).set_body_json(first))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        let second = json!({"items": [{"thread_id": "b"}], "next_cursor": null});
        Mock::given(method("POST"))
            .and(body_partial_json(json!({"cursor": "c1"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(second))
            .expect(1)
            .mount(&server)
            .await;
        let client = client(&server);
        let page = client.list_threads(&project(), since(), None).await.unwrap();
        assert_eq!(page.next_cursor.as_deref(), Some("c1"));
        let next = client.list_threads(&project(), since(), page.next_cursor.as_deref()).await.unwrap();
        assert_eq!(next.threads[0].id.0, "b");
    }

    #[tokio::test]
    async fn list_turns_selects_every_field_and_reads_turns_oldest_first() {
        let server = MockServer::start().await;
        let items = json!({"items": [
            {"trace_id": "late", "start_time": "2026-03-01T10:05:00Z", "end_time": "2026-03-01T10:05:01Z"},
            {"trace_id": "early", "start_time": "2026-03-01T10:00:00Z", "end_time": "2026-03-01T10:00:01Z"}
        ], "next_cursor": null});
        let mut mock = Mock::given(method("GET")).and(path("/api/v2/threads/th-1/traces")).and(query_param("project_id", PROJECT));
        for field in TURN_FIELDS {
            mock = mock.and(query_param("selects", field));
        }
        mock.respond_with(ResponseTemplate::new(200).set_body_json(items)).expect(1).mount(&server).await;
        let page = client(&server).list_turns(&project(), &ThreadId("th-1".into())).await.unwrap();
        let ids: Vec<&str> = page.turns.iter().map(|turn| turn.trace_id.as_str()).collect();
        assert_eq!((ids, page.truncated), (vec!["early", "late"], false));
    }

    #[tokio::test]
    async fn list_turns_stops_at_the_page_cap_and_says_truncated() {
        let server = MockServer::start().await;
        let endless = json!({"items": [{"trace_id": "t"}], "next_cursor": "more"});
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(200).set_body_json(endless)).expect(5).mount(&server).await;
        let page = client(&server).list_turns(&project(), &ThreadId("th".into())).await.unwrap();
        assert_eq!((page.turns.len(), page.truncated), (5, true));
    }

    #[tokio::test]
    async fn statuses_map_to_typed_errors() {
        let cases = [
            (401, ApiError::KeyRejected),
            (403, ApiError::KeyRejected),
            (429, ApiError::RateLimited),
            (500, ApiError::Status(500)),
            (501, ApiError::Unavailable(501)),
        ];
        for (status, expected) in cases {
            let server = MockServer::start().await;
            Mock::given(method("POST")).respond_with(ResponseTemplate::new(status)).mount(&server).await;
            let err = client(&server).list_threads(&project(), since(), None).await.unwrap_err();
            assert_eq!(err, expected, "{status}");
        }
    }

    #[test]
    fn an_api_url_that_would_send_the_key_in_clear_is_refused() {
        for url in
            ["http://api.smith.langchain.com", "http://localhost.evil.com", "http://localhost@evil.com", "ftp://localhost", "nonsense"]
        {
            assert!(matches!(Client::new(url, "k"), Err(ApiError::InsecureUrl(_))), "{url}");
        }
        assert!(Client::new("https://api.smith.langchain.com", "k").is_ok());
        assert!(Client::new("http://127.0.0.1:8080", "k").is_ok());
    }

    #[tokio::test]
    async fn a_redirect_is_not_followed_so_the_key_never_reaches_another_host() {
        let elsewhere = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": []})))
            .expect(0)
            .mount(&elsewhere)
            .await;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(307).insert_header("location", format!("{}/steal", elsewhere.uri())))
            .mount(&server)
            .await;
        let err = client(&server).list_threads(&project(), since(), None).await.unwrap_err();
        assert_eq!(err, ApiError::Status(307));
    }

    #[test]
    fn debug_never_prints_the_key() {
        assert!(!format!("{:?}", Client::new("https://x.test", "secret-value").unwrap()).contains("secret-value"));
    }

    #[tokio::test]
    async fn read_run_url_sends_the_project_and_the_trace_and_returns_the_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/runs/tr-1/url"))
            .and(query_param("project_id", PROJECT))
            .and(query_param("trace_id", "tr-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"url": "https://smith.example/r/tr-1"})))
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(client(&server).read_run_url(&project(), "tr-1").await.unwrap(), "https://smith.example/r/tr-1");
    }
}
