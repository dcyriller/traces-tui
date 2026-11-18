//! The fix action: the command line to run, and running it in the repository.
use crate::config::Config;
use crate::langsmith::url::{UrlParts, fill};
use crate::marks::Mark;
use crate::prompt::{DEFAULT_TEMPLATE, MarkedThread, render};
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

/// The command to run: its arguments with the prompt last, and where to run it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixPlan {
    /// The program, its arguments and the prompt.
    pub argv: Vec<String>,
    /// The repository the command runs in.
    pub cwd: PathBuf,
    /// The prompt, also the last element of `argv`.
    pub prompt: String,
    /// Placeholders of the template that nothing fills.
    pub unknown_placeholders: Vec<String>,
}

fn link_of(config: &Config, mark: &Mark) -> Option<String> {
    let org_id = config.org_id.as_deref()?;
    let project_id = mark.project_id.as_deref()?;
    let trace_id = mark.last_trace_id.as_deref()?;
    Some(fill(&config.web_url_template, UrlParts { org_id, project_id, thread_id: &mark.thread_id.0, trace_id }))
}

fn marked_thread(config: &Config, mark: &Mark) -> MarkedThread {
    MarkedThread { url: link_of(config, mark), thread_id: mark.thread_id.0.clone(), marked_at: mark.marked_at, note: mark.note.clone() }
}

fn is_found(program: &str, path_var: Option<&std::ffi::OsStr>) -> bool {
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    path_var.is_some_and(|dirs| std::env::split_paths(dirs).any(|dir| dir.join(program).is_file()))
}

/// Builds the command for `marks` of `project`; fails early when the repository or the program is missing.
pub fn plan(config: &Config, project: &str, marks: &[&Mark]) -> Result<FixPlan> {
    let mut argv = shell_words::split(&config.fix_command)?;
    let Some(program) = argv.first() else { bail!("fix_command is empty") };
    if !config.repo_dir.is_dir() {
        bail!("repo_dir {} is not a folder", config.repo_dir.display());
    }
    if !is_found(program, std::env::var_os("PATH").as_deref()) {
        bail!("{program} was not found on PATH");
    }
    let threads: Vec<MarkedThread> = marks.iter().map(|mark| marked_thread(config, mark)).collect();
    let template = config.prompt_template.as_deref().unwrap_or(DEFAULT_TEMPLATE);
    let rendered = render(template, project, &threads);
    argv.push(rendered.text.clone());
    Ok(FixPlan { argv, cwd: config.repo_dir.clone(), prompt: rendered.text, unknown_placeholders: rendered.unknown })
}

/// Runs the command with the terminal; no shell is involved, so the prompt arrives as one argument.
pub fn launch(plan: &FixPlan) -> Result<ExitStatus> {
    let Some((program, args)) = plan.argv.split_first() else { bail!("the fix command is empty") };
    Ok(Command::new(program).args(args).current_dir(&plan.cwd).status()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marks::tests::mark;

    fn config(command: &str, dir: &Path) -> Config {
        Config { fix_command: command.to_owned(), repo_dir: dir.to_path_buf(), ..Config::default() }
    }

    #[test]
    fn the_prompt_is_the_last_argument_and_the_repo_the_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan(&config("true --flag", dir.path()), "app-production", &[&mark("a", "n")]).unwrap();
        assert_eq!(plan.argv[..2], ["true", "--flag"]);
        assert_eq!(plan.argv.last(), Some(&plan.prompt));
        assert_eq!(plan.cwd, dir.path());
    }

    #[test]
    fn a_note_with_quotes_substitutions_and_newlines_arrives_as_one_intact_argument() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("argument.txt");
        let command = format!("sh -c 'printf %s \"$1\" > {}' sh", out.display());
        let nasty = mark("a", "he said \"no\" $(touch pwned) `id`\n'quoted'");
        let plan = plan(&config(&command, dir.path()), "p", &[&nasty]).unwrap();
        assert!(launch(&plan).unwrap().success());
        let received = std::fs::read_to_string(&out).unwrap();
        assert_eq!(received, plan.prompt);
        assert!(received.contains("$(touch pwned) `id` 'quoted'"));
        assert!(!dir.path().join("pwned").exists());
    }

    #[test]
    fn a_missing_repo_dir_or_program_fails_before_launching() {
        let dir = tempfile::tempdir().unwrap();
        assert!(plan(&config("true", &dir.path().join("none")), "p", &[]).is_err());
        assert!(plan(&config("no-such-program-xyz", dir.path()), "p", &[]).is_err());
        assert!(plan(&config("", dir.path()), "p", &[]).is_err());
    }

    #[test]
    fn launch_reports_the_exit_status() {
        let dir = tempfile::tempdir().unwrap();
        let ok = plan(&config("true", dir.path()), "p", &[&mark("a", "")]).unwrap();
        let failing = plan(&config("false", dir.path()), "p", &[&mark("a", "")]).unwrap();
        assert!(launch(&ok).unwrap().success());
        assert!(!launch(&failing).unwrap().success());
    }

    #[test]
    fn without_an_org_id_a_mark_has_no_link() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config { org_id: None, ..config("true", dir.path()) };
        assert!(plan(&config, "p", &[&mark("a", "n")]).unwrap().prompt.contains("(no link)"));
    }

    #[test]
    fn a_mark_with_an_org_id_has_a_link() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            org_id: Some("org-1".into()),
            web_url_template: "https://x/{org_id}/{project_id}/{trace_id}".into(),
            ..config("true", dir.path())
        };
        assert!(plan(&config, "p", &[&mark("a", "n")]).unwrap().prompt.contains("https://x/org-1/proj-1/trace-a"));
    }

    #[test]
    fn a_mark_without_a_project_id_has_no_link() {
        let dir = tempfile::tempdir().unwrap();
        let bare = Mark { project_id: None, ..mark("a", "n") };
        assert!(plan(&config("true", dir.path()), "p", &[&bare]).unwrap().prompt.contains("(no link)"));
    }
}
