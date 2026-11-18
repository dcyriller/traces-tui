# traces

Triage your agent's LangSmith traces from the terminal, mark the conversations that went wrong, and hand them to Claude Code to fix.

- **Newest threads first.** `traces` lists the latest threads of a LangSmith project, and `enter` shows a thread's turns: the employee message, the agent's reply, the route it took and any error.
- **Mark with a note.** `m` marks a thread "to fix" with a line saying what is wrong.
- **One key to fix.** `F` starts `claude` in your repository with a prompt that lists the marked threads, their LangSmith links and your notes.
- **Terminal only.** Traces can hold real employee data, so nothing is cached on disk and no message text is stored or logged.

```text
 app-production  3 loaded  filter all  more
▎2h ago    How many parental leave days do I get in Spain?     3  E F *
 5h ago    Can I carry my unused days over to next year?       1
 2025-11-17 Where do I send my sick note?                      2    F v
 ? keys  enter open  m mark  F fix  / search  q quit
```

`E` means a turn errored, `F` means the thread has feedback, `*` is a mark still to fix and `v` is a mark already sent.

## Install

Rust 1.92 or newer:

```sh
cargo install --git https://github.com/dcyriller/traces-tui
```

Or from a clone:

```sh
git clone https://github.com/dcyriller/traces-tui
cd traces-tui
cargo install --path .
```

## Quick start

```sh
export LANGSMITH_API_KEY=...
traces
```

The key comes from the environment only.
`traces` reads the project in your config (`app-production` by default) and lists the threads active in the last 7 days.
Press `?` to see every key.

Without the key, the TUI still opens: you can read and edit your marks, and `traces fix` still works.

## Config

The config file is `~/.config/traces/config.toml`, or the path in `$TRACES_CONFIG`, or the one you pass with `--config`.
A missing file means the defaults below.
An unknown key is an error, and so is an API key: put that in the environment.

```toml
project      = "app-production"
api_url      = "https://api.smith.langchain.com"
repo_dir     = "/Users/cyrille/Documents/enroll/app"
fix_command  = "claude"
since        = "7d"
org_id       = "398a5582-4144-4897-ba96-149d296b0ba7"
web_url_template = "https://smith.langchain.com/o/{org_id}/projects/p/{project_id}/r/{trace_id}"
theme        = "default"
```

| Key | What it does |
| --- | --- |
| `project` | The LangSmith project, matched by exact name. |
| `api_url` | The API base. It must be `https`, or `http` on `localhost` or `127.0.0.1`, so the key never travels in clear. |
| `repo_dir` | Where the fix command runs. |
| `fix_command` | The command to run, split like a shell would. The prompt is added as its last argument, and no shell is involved. |
| `since` | How far back to list thread activity: `5m`, `12h` or `7d`. `--since` overrides it. |
| `org_id` | The LangSmith organization id used in links. |
| `web_url_template` | The link to a trace. It can use `{org_id}`, `{project_id}`, `{thread_id}` and `{trace_id}`. |
| `theme` | A [vtui](https://github.com/vmeyet/vtui) theme: `default`, `dracula`, `catppuccin`, `catppuccin-latte`, `rosepine`, `rosepine-dawn`, `nord`, `tokyonight` or `monokai`. |
| `prompt_template` | Replaces the fix prompt. It can use `{count}`, `{project}` and `{threads}`. |

The default link shape has not been checked against the live web app.
If a link opens the wrong page, fix `web_url_template`, and compare with `traces url <thread id>`, which asks LangSmith for the real link.

## Keys

| Key | Does |
| --- | --- |
| `j` `k` | Move, or scroll in a thread. |
| `g` `G` | First and last thread. `G` loads more. |
| `enter` | Open the thread. |
| `m` | Mark the thread or edit its note. |
| `u` | Unmark, after a `y`. |
| `f` | Cycle the filter: all, marked, todo. |
| `/` | Search the first messages. |
| `r` | Reload from the first page. |
| `n` | Load the next page (in a thread: next turn). |
| `F` | Launch the fix command on the marks still to fix. |
| `o` | Open the thread in the browser. |
| `ctrl-d` `ctrl-u` | Half a page in a thread. |
| `p` | Previous turn, in a thread. |
| `esc` `q` | Back, from a thread. |
| `?` | Show the keys. |
| `q` `ctrl-c` | Quit. |

## Fixing

`F` shows `Launch claude on 3 threads in /path/to/repo? y/n`.
On `y`, traces gives the terminal to `claude`, started in `repo_dir` with the prompt as its argument, and takes it back when `claude` exits.
If it exits 0, the marks become sent (`v`).
If not, they stay to fix and the status line shows the exit code.
Sent means handed over, not that the fix landed.

The prompt holds thread ids, LangSmith links and your notes, never message text.

## Commands

```sh
traces marks [--json] [--status todo|sent]   # print the marks of the project
traces fix [--dry-run] [--yes] [--all]       # launch the fix command, or print what it would run
traces url <thread id>                       # print the link LangSmith gives for the thread's latest turn
```

`--project`, `--config` and `--since` work on every command.

## What it keeps

- **The key** is read from `LANGSMITH_API_KEY` and nowhere else.
  It is never written to a file or a log.
- **Marks** go to `~/.local/share/traces/marks.json` (or under `$XDG_DATA_HOME`).
  A mark holds the thread id, the latest trace id, the project, your note and two timestamps.
  Your note is whatever you type, so keep personal data out of it.
- **The log** at `~/.local/state/traces/traces.log` holds events, never message text or response bodies.
- **Threads and turns** live in memory only.

## When something is off

| You see | It means |
| --- | --- |
| `LANGSMITH_API_KEY is not set` | Export the key and start again. |
| `key rejected by LangSmith` | A 401 or 403: the key is wrong or lacks access. |
| `project app-production not found` | No project has exactly that name: check `project`. |
| `no threads since 7d, try --since 30d` | The window is empty. |
| `threads API unavailable (501)` | A self-hosted LangSmith older than v0.16 has no threads endpoints. |
| `network error, press r` | A timeout or a refused connection. The loaded rows stay. |
