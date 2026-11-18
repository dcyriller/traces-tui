//! The config file: defaults, the lookup order and what it refuses.
use anyhow::{Context, Result, bail};
use chrono::Duration;
use serde::Deserialize;
use std::path::{Path, PathBuf};

const KEY_NAMES: [&str; 4] = ["api_key", "langsmith_api_key", "token", "secret"];

/// The settings of `config.toml`; a missing file means these defaults.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// The LangSmith project name to read.
    pub project: String,
    /// The LangSmith API base URL.
    pub api_url: String,
    /// The repository the fix command runs in.
    pub repo_dir: PathBuf,
    /// The command to launch, split like a shell would; the prompt is appended as its last argument.
    pub fix_command: String,
    /// How far back thread activity is listed.
    pub since: String,
    /// The LangSmith organization id used in links; without it no link is built.
    pub org_id: Option<String>,
    /// The link to a trace, with `{org_id}`, `{project_id}`, `{thread_id}` and `{trace_id}`.
    pub web_url_template: String,
    /// The name of a vtui theme.
    pub theme: String,
    /// Replaces the default fix prompt.
    pub prompt_template: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            project: "app-production".to_owned(),
            api_url: "https://api.smith.langchain.com".to_owned(),
            repo_dir: std::env::current_dir().unwrap_or_else(|_error| PathBuf::from(".")),
            fix_command: "claude".to_owned(),
            since: "7d".to_owned(),
            org_id: None,
            web_url_template: "https://smith.langchain.com/o/{org_id}/projects/p/{project_id}/r/{trace_id}".to_owned(),
            theme: "default".to_owned(),
            prompt_template: None,
        }
    }
}

impl Config {
    /// Reads the file at `path`; a file that does not exist gives the defaults.
    pub fn load(path: &Path) -> Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
        };
        Self::parse(&text).with_context(|| format!("in {}", path.display()))
    }

    fn parse(text: &str) -> Result<Self> {
        let table: toml::Table = text.parse()?;
        if let Some(name) = KEY_NAMES.iter().find(|name| table.contains_key(**name)) {
            bail!("`{name}` does not belong in the config file: set LANGSMITH_API_KEY in the environment");
        }
        Ok(table.try_into()?)
    }
}

/// Where the config file is: `--config`, then `$TRACES_CONFIG`, then `~/.config/traces/config.toml`.
pub fn config_path(flag: Option<&Path>, env: Option<&Path>, home: &Path) -> PathBuf {
    flag.or(env).map_or_else(|| home.join(".config/traces/config.toml"), Path::to_path_buf)
}

/// Reads `5m`, `12h` or `7d` as a duration.
pub fn parse_since(text: &str) -> Result<Duration> {
    let text = text.trim();
    let split = text.len().saturating_sub(1);
    let (digits, unit) = text.split_at_checked(split).unwrap_or(("", ""));
    let count: i64 =
        digits.parse().ok().filter(|count| *count > 0).with_context(|| format!("`{text}` is not a duration like 5m, 12h or 7d"))?;
    match unit {
        "m" => Ok(Duration::minutes(count)),
        "h" => Ok(Duration::hours(count)),
        "d" => Ok(Duration::days(count)),
        _ => bail!("`{text}` is not a duration like 5m, 12h or 7d"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_gives_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Config::load(&dir.path().join("none.toml")).unwrap(), Config::default());
    }

    #[test]
    fn the_defaults_name_no_organization_and_run_in_the_current_folder() {
        let config = Config::default();
        assert_eq!(config.org_id, None);
        assert_eq!(config.repo_dir, std::env::current_dir().unwrap());
    }

    #[test]
    fn a_file_overrides_only_the_keys_it_sets() {
        let config = Config::parse("project = \"staging\"\nfix_command = \"true\"").unwrap();
        assert_eq!(config.project, "staging");
        assert_eq!(config.fix_command, "true");
        assert_eq!(config.since, "7d");
    }

    #[test]
    fn an_unknown_key_is_an_error() {
        assert!(Config::parse("projet = \"x\"").is_err());
    }

    #[test]
    fn a_key_in_the_file_is_refused_and_points_at_the_environment() {
        let err = Config::parse("api_key = \"x\"").unwrap_err().to_string();
        assert!(err.contains("LANGSMITH_API_KEY"), "{err}");
    }

    #[test]
    fn the_flag_beats_the_environment_beats_the_home_default() {
        let home = Path::new("/home/nina");
        let flag = Path::new("/a.toml");
        let env = Path::new("/b.toml");
        assert_eq!(config_path(Some(flag), Some(env), home), flag);
        assert_eq!(config_path(None, Some(env), home), env);
        assert_eq!(config_path(None, None, home), Path::new("/home/nina/.config/traces/config.toml"));
    }

    #[test]
    fn since_reads_minutes_hours_and_days() {
        assert_eq!(parse_since("5m").unwrap(), Duration::minutes(5));
        assert_eq!(parse_since("12h").unwrap(), Duration::hours(12));
        assert_eq!(parse_since("7d").unwrap(), Duration::days(7));
    }

    #[test]
    fn since_refuses_garbage() {
        for text in ["", "d", "7", "7w", "-1d", "0h", "abc", "é"] {
            assert!(parse_since(text).is_err(), "{text}");
        }
    }
}
