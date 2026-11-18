//! What a run needs from outside: the config, the key and the paths on disk.
use crate::cli::Cli;
use crate::config::{Config, config_path};
use anyhow::{Context, Result};
use std::fmt;
use std::path::PathBuf;

/// The variables a run reads, gathered once so tests can build their own.
#[derive(Clone, Debug, Default)]
pub struct Env {
    /// `$HOME`.
    pub home: PathBuf,
    /// `$XDG_DATA_HOME`.
    pub xdg_data_home: Option<PathBuf>,
    /// `$TRACES_CONFIG`.
    pub traces_config: Option<PathBuf>,
    /// `$LANGSMITH_API_KEY`.
    pub api_key: Option<String>,
}

impl Env {
    /// Reads the process environment.
    pub fn capture() -> Result<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from).context("$HOME is not set")?;
        Ok(Self {
            home,
            xdg_data_home: std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|path| !path.as_os_str().is_empty()),
            traces_config: std::env::var_os("TRACES_CONFIG").map(PathBuf::from).filter(|path| !path.as_os_str().is_empty()),
            api_key: std::env::var("LANGSMITH_API_KEY").ok().filter(|key| !key.trim().is_empty()),
        })
    }
}

/// Raised when an API call is made without `LANGSMITH_API_KEY`.
#[derive(Debug)]
pub struct MissingApiKey;

impl fmt::Display for MissingApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LANGSMITH_API_KEY is not set")
    }
}

impl std::error::Error for MissingApiKey {}

/// The config with the flags applied, the key and the paths the run uses.
#[derive(Clone)]
pub struct Ctx {
    /// The settings.
    pub config: Config,
    /// Where the marks are stored.
    pub marks_path: PathBuf,
    /// Where the log goes.
    pub log_path: PathBuf,
    api_key: Option<String>,
}

impl fmt::Debug for Ctx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ctx").field("config", &self.config).field("marks_path", &self.marks_path).finish_non_exhaustive()
    }
}

impl Ctx {
    /// Loads the config named by the flags and `env`, then lets `--project` and `--since` override it.
    pub fn build(cli: &Cli, env: Env) -> Result<Self> {
        let path = config_path(cli.config.as_deref(), env.traces_config.as_deref(), &env.home);
        let mut config = Config::load(&path)?;
        if let Some(project) = &cli.project {
            config.project.clone_from(project);
        }
        if let Some(since) = &cli.since {
            config.since.clone_from(since);
        }
        let data_home = env.xdg_data_home.unwrap_or_else(|| env.home.join(".local/share"));
        Ok(Self {
            config,
            marks_path: data_home.join("traces/marks.json"),
            log_path: env.home.join(".local/state/traces/traces.log"),
            api_key: env.api_key,
        })
    }

    /// The key for an API call, or `MissingApiKey`.
    pub fn api_key(&self) -> Result<&str, MissingApiKey> {
        self.api_key.as_deref().ok_or(MissingApiKey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn env(home: &std::path::Path) -> Env {
        Env { home: home.to_path_buf(), ..Env::default() }
    }

    #[test]
    fn a_missing_key_is_a_typed_error_raised_on_use() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Ctx::build(&Cli::parse_from(["traces"]), env(dir.path())).unwrap();
        assert!(ctx.api_key().is_err());
    }

    #[test]
    fn flags_override_the_file_and_paths_follow_the_environment() {
        let dir = tempfile::tempdir().unwrap();
        let cli = Cli::parse_from(["traces", "--project", "staging", "--since", "30d"]);
        let ctx = Ctx::build(&cli, Env { xdg_data_home: Some("/data".into()), api_key: Some("k".into()), ..env(dir.path()) }).unwrap();
        assert_eq!((ctx.config.project.as_str(), ctx.config.since.as_str()), ("staging", "30d"));
        assert_eq!(ctx.marks_path, std::path::Path::new("/data/traces/marks.json"));
        assert_eq!(ctx.api_key().unwrap(), "k");
    }

    #[test]
    fn debug_never_prints_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Ctx::build(&Cli::parse_from(["traces"]), Env { api_key: Some("secret-value".into()), ..env(dir.path()) }).unwrap();
        assert!(!format!("{ctx:?}").contains("secret-value"));
    }
}
