# traces-tui: v1 implementation plan

Crate `traces-tui`, binary `traces`. Rust 2024, rust-version 1.92. Target under 2500 lines of Rust including tests.
Plan only. Every API fact below was read from `openapi.json` and the enroll backend, not from a live call. Per-turn feedback is not selectable, so detail shows thread-level feedback only.

## Outcome

- `traces` opens the newest threads of a LangSmith project, shows a thread's turns, marks threads "to fix" with a note, and launches `claude` in the enroll repo with a prompt listing the marked threads.
- Prod traces hold real employee data. The tool shows them in the terminal only. Marks store thread id, last trace id, project, note and times, never message text (the note is whatever the user types). The fix prompt carries thread ids, LangSmith URLs and the note, never message text.

## Grounding

| Fact | Source |
|---|---|
| Thread id is the Request id, set as `metadata.thread_id` on the root run | `apps/backend/src/agent/annotations/traceAgentTurn.ts` (name `agent.turn`) |
| Root inputs are `{ "input": "<employee message>" }` | same file, `processInputs` |
| Root outputs are `{ kind, ...outcome, routeTaken }`; the spread puts the outcome's own `kind` last, so `outputs.kind` is `answer`, `clarify`, `close`, `escalate`, `reply` or `confirm`, and `failed` only when the turn failed (no outcome fields) | `toTracedOutcomeAndRoute` in `traceAgentTurn.ts`; `TurnResult` in `domain/agent/useCases/runAgentTurn.ts` |
| `routeTaken` is a reader key, `search`, or a workflow key | `RequestRoute` in `agent/types.ts` |
| Nested graph run is `agent.reason` (not read in v1) | `agent/search/runSearchTurn.ts` |
| Reply text: `outputs.content` for `answer`, `clarify`, `close` and `reply`. `escalate` has `opsContent` (note to HR, not a reply) and `draftedAnswer` (string or null). `confirm` has `card` and `submission`, no text | `agent/search/turnOutcome/types.ts`, `agent/outcome/types.ts`, `agent/submission/collect/collectState.ts` |

## Decision: use vtui v0.9.0 for screen, theme, help, field

`vtui = { git = "https://github.com/vmeyet/vtui", tag = "v0.9.0" }`, same pin as pgbg-tui. Its API, read from source:

| Piece | Use | Why |
|---|---|---|
| `screen::Screen::enter_without_mouse()` returns `(DefaultTerminal, Screen)`, `leave()` | yes | installs the panic hook that restores the terminal, plus paste and kitty keys |
| `Screen::lend(&self, &mut terminal, run)` | yes | the "suspend like $EDITOR" the fix action needs: restores the tty, runs the sync closure, re-enters, forces a full repaint. pgbg-tui calls it inside `tokio::task::block_in_place` (`tui/mod.rs:102`) and builds a new `EventStream` afterwards (`:181`) |
| `theme::Theme` (`Theme::named(&str) -> Option<Theme>`, `Theme::default()`, fields `accent`, `muted`, `faded`, `danger`, `warn`, `success`, `border`) | yes | consistent colours with the siblings, no hardcoded colours |
| `help::{Help, Section, Row, draw, last_row}` | yes | `?` overlay: `draw(f, &Theme, area, Help, &[Section])`, `Row { keys: String, does: &'static str }` so key help text is `'static` |
| `field::Field` (`new`, `text`, `insert`, `backspace`, `delete_word`, `left`, `right`, `take`, `paste`) | yes | one-line note input and the search box; the loop must handle `Event::Paste` |
| `text::truncate(&str, usize) -> String` | yes | cell truncation by display width |
| `editor::edit` (long note in `$EDITOR`), `palette`, `fuzzy`, `complete`, `update`, `usage`, `click`, `pane`, `toward` | no | out of v1 scope; `edit` is a v2 option for long notes via `lend` |

Cost: a git dependency (network at first build, `Cargo.lock` pins the commit) and a few transitive crates (`base64`, `libc`, `tempfile`). Accepted: plain ratatui would mean re-writing the terminal restore, paste and panic-hook code vtui already has. `Theme` is `Copy`. The panic hook runs before an abort, so `panic = "abort"` in `[profile.release]` (pgbg-tui's) still restores the terminal; `run` also calls `screen.leave()` on every `Err` path.

## Cargo.toml

- `[package] name = "traces-tui"`, `[[bin]] name = "traces" path = "src/main.rs"`, edition 2024, rust-version 1.92.
- Dependencies: `anyhow`, `chrono` (clock, serde), `clap` 4.5 derive, `crossterm` 0.29 `event-stream`, `futures-util` (alloc), `ratatui` 0.30, `reqwest` 0.12 (default-features off, `rustls-tls`, `json`), `serde` derive, `serde_json`, `shell-words` (split `fix_command`), `tempfile`, `tokio` (rt-multi-thread, macros, sync, time, process), `toml`, `tracing` + `tracing-subscriber` (file logger only), `vtui`.
- Dev: `wiremock`, `tempfile`, `tokio` test-util.
- `[lints]`: copy pgbg-tui's block verbatim (pedantic warn, `unwrap_used`, `expect_used`, `unsafe_code = "forbid"`, `unreachable_pub`, `missing_docs`, the same allows). `clippy.toml`: `allow-unwrap-in-tests`, `allow-expect-in-tests`. Also copy `rustfmt.toml`, `rust-toolchain.toml` (stable, rustfmt + clippy).
- No `reqwest` default TLS (native-tls), so the binary stays rustls. First build needs network for the `vtui` git dependency; tests do not.

## Module layout

| File | One line |
|---|---|
| `main.rs` | parses `Cli`, builds `Ctx`, dispatches to a subcommand or the TUI |
| `lib.rs` | module declarations, `run(cli) -> Result<()>` |
| `cli.rs` | clap derive: `--project`, `--config`, `--since`, subcommands `marks`, `fix`, `url` |
| `config.rs` | `Config` (toml, `$TRACES_CONFIG` or `~/.config/traces/config.toml`), defaults, `load()` |
| `ctx.rs` | `Ctx { config, api_key, marks_path }`, reads `LANGSMITH_API_KEY` from env only |
| `langsmith/mod.rs` | `Client` (reqwest), `list_threads`, `list_turns`, `resolve_project`, `read_run_url` |
| `langsmith/wire.rs` | serde structs of the four responses, only the fields read |
| `langsmith/url.rs` | fills `web_url_template` (`org_id`, `project_id`, `thread_id`, `trace_id`) |
| `model.rs` | domain types: `ProjectId`, `ThreadId`, `Thread`, `Turn`, `TurnStatus`, `Feedback` |
| `turn.rs` | `to_turn(wire) -> Turn`: pulls `input`, reply text, `routeTaken`, `kind` out of JSON values |
| `marks.rs` | `Mark`, `MarkStatus`, `Marks` store: load, upsert, remove, mark_sent, atomic save |
| `prompt.rs` | `render(template, &[MarkedThread]) -> String`, pure |
| `fix.rs` | `FixPlan` (argv, cwd, prompt), `plan(config, marks) -> FixPlan`, `launch(plan) -> ExitStatus` |
| `app.rs` | pure state `App` and `update(App, Msg) -> (App, Vec<Effect>)` |
| `keys.rs` | `Key -> Msg` per mode, and the `help::Section` list built from the same table |
| `event_loop.rs` | tokio loop: crossterm `EventStream` + `mpsc<Msg>`; runs `Effect`s (spawn fetch, save marks, lend terminal) |
| `view/mod.rs` | `draw(frame, &App, Theme)` picks the screen |
| `view/list.rs` | thread list table, status line, search/filter chips |
| `view/detail.rs` | scrollable turns |
| `view/note.rs` | note input popup and confirm popup |
| `commands.rs` | non-TUI subcommands `marks` (print table or `--json`) `fix` (`--dry-run` prints the prompt) and `url <thread>` (prints the API's link) |

Rules held (revu-tui style): `app.rs` and `view/` do no I/O; side effects only in `event_loop.rs`, `langsmith/`, `marks.rs` (disk), `fix.rs` (process).

## Data types

```
ThreadId(String)            // uuid, newtype
ProjectId(String)           // uuid, newtype
Thread { id, started_at, last_active_at, last_trace_id, first_input: Option<String>, turn_count: u32,
         errored_turns: u32, last_error: Option<String>, feedback: Vec<FeedbackKey> }
FeedbackKey { key: String, n: u32, avg: Option<f64> }   // from feedback_stats, see below
Turn { trace_id, started_at, latency_s: Option<f64>, status: TurnStatus,
       input: Option<String>, reply: Option<String>, route: Option<String>, error: Option<String> }
TurnStatus = Ok | Failed | Errored | Running   // Failed = outputs.kind == "failed"; Errored = error_preview set; Running = end_time null
Mark { thread_id, last_trace_id, project, note, marked_at, status: MarkStatus, sent_at: Option<DateTime> }
MarkStatus = Todo | Sent   // Sent means launched: claude exited 0, not that the fix landed
```

Thread titles and turn text are `Option` because the previews are truncated server-side and may be absent.

## LangSmith calls

Base `https://api.smith.langchain.com` (config `api_url`), header `x-api-key: $LANGSMITH_API_KEY`, `Accept: application/json`. Per-request timeout 20 s (`reqwest` builder), no retry in v1 beyond a manual `r` refresh. `api_url` must be `https`, or `http` only for `localhost`/`127.0.0.1` (wiremock tests), so the key never travels in clear. Errors map as in the error table below.

### 1. Resolve project name to UUID (once at startup, cached in memory)

`GET /api/v1/sessions?name=app-production&limit=100` returns an array of `TracerSession` (both `name` and `name_contains` exist, the spec does not say whether `name` is exact). Read `id`, `name`. Keep the item whose `name` equals the requested one exactly, client-side. No exact match is an error: "project not found".

### 2. List threads, newest first, with paging

`POST /api/v2/threads/query`

```json
{ "project_id": "<uuid>", "page_size": 50, "min_start_time": "<now - since>", "cursor": "<next_cursor or omitted>" }   // + header Accept: application/json
```

- `min_start_time` defaults to one day ago when omitted (the known trap), so the client always sends it. `--since` default `7d`, parsed by a small `5m|12h|7d` parser in `config.rs`, formatted RFC3339 UTC. All request fields above exist in `threads.QueryThreadsRequestBody`; `max_start_time` is left out (defaults to now).
- No order parameter exists: the spec says items are "sorted by the thread's most recent activity", so v1 relies on that and re-sorts the loaded page set by `max_start_time` descending as a guard.
- Paging: `page_size` is 1..100; `next_cursor` is null on the last page. Load one page at start, the next page when the selection is within 5 rows of the end (`n`/`G` also trigger it). The response may hold fewer rows than `page_size` while `next_cursor` is set, so continue on `next_cursor`, never on row count.
- Fields read from each `items[]` entry (all in `threads.ThreadListItem`): `thread_id`, `min_start_time`, `max_start_time`, `count` (turns inside the window, so the title says "turns in window"), `first_inputs`, `num_errored_turns`, `last_error`, `feedback_stats`, `last_trace_id`. `last_outputs` is not read.
- `first_inputs` is a truncated preview string, likely the JSON of the inputs (`{"input":"..."}`), so a cut preview may not parse; `turn::preview_text` tries JSON and takes `input`, else shows the raw string cut to width. Shape unverified without a live call.
- `feedback_stats` is a map `key -> RunFeedbackStat` (`additionalProperties` of `query.RunFeedbackStat`). Fields: `n` (points), `avg`, `min`, `max`, `stdev`, `values` (label counts), `comments`, `sources`, `errors`, `contains_thread_feedback`. There is no positive/negative field. v1 reads only `n` and `avg`, shows `F` when any key has `n > 0`, and lists `key n avg` in the detail title. It never reads `comments` (user text).

### 3. Load one thread's turns

`GET /api/v2/threads/{thread_id}/traces?project_id=<uuid>&page_size=100&selects=START_TIME&selects=END_TIME&selects=LATENCY&selects=INPUTS&selects=OUTPUTS&selects=ERROR_PREVIEW&selects=TRACE_ID&selects=TURN_NUMBER` (`project_id` is required; page_size 1..100, default 20; every `selects` value is in `ThreadTraceSelectField`)

- Response `{ items: [ThreadTraceListItem], next_cursor }`. `selects` is a repeated query parameter; `selects` values are upper snake case, and without them only ids come back (the same trap as runs/query).
- Follow `next_cursor` up to 5 pages (500 turns), then show "truncated" in the title.
- Fields read per item: `trace_id`, `start_time`, `end_time` (null while running), `latency` (seconds, float), `inputs`, `outputs`, `error_preview`, `turn_number`. Sort by `start_time` ascending client-side. `NAME` is not selected (always `agent.turn`).
- `inputs`/`outputs` are JSON objects (`serde_json::Value`). `turn::to_turn` reads `inputs.input`, `outputs.kind`, `outputs.routeTaken`, and the reply from `outputs.content` when `kind` is `answer`, `clarify`, `close` or `reply`. `escalate` shows "handed off to HR" plus `draftedAnswer` when set, never `opsContent` by default. `confirm` shows "(confirmation card)". Any other shape falls back to the compact JSON of `outputs` minus `kind` and `routeTaken`, cut to the pane width.
- Chosen over `GET /api/v1/runs/threads/{id}` (returns only `previews` strings, no status or error) and over `POST /api/v1/runs/query` (needs `session` UUIDs, a `start_time` bound, `is_root`, an upper snake `select` list, and a filter `eq(name,"agent.turn")`; more traps for the same data). Both v2 endpoints exist in the spec (cloud has them; self-hosted needs LangSmith v0.16+, else 501). A 501 or 404 shows "threads API unavailable"; the `runs/query` fallback is not built in v1.

### Thread URL (for the prompt and the `o` key)

The spec has `GET /api/v2/runs/{run_id}/url?project_id=&trace_id=` returning `{ "url": string }`, the authoritative link, but one call per link is too heavy for the prompt. v1 builds links locally from `web_url_template`, default `https://smith.langchain.com/o/{org_id}/projects/p/{project_id}/r/{trace_id}` with `{org_id}`, `{project_id}`, `{thread_id}`, `{trace_id}`. `trace_id` is the thread's `last_trace_id`, so the link opens the latest turn. `org_id` defaults to `398a5582-4144-4897-ba96-149d296b0ba7` (config `org_id`). The default path shape is UNVERIFIED (no live call made): step 8's manual check opens one link, and `traces url <thread_id>` (one call to the `/url` endpoint) prints the real one for comparison. Only `https` URLs are opened.

## Config

`~/.config/traces/config.toml`, or the path in `$TRACES_CONFIG`, or `--config`. Missing file means defaults. Unknown keys are an error (`deny_unknown_fields`). The API key is never read from this file; a key in the file is rejected with a message pointing at the env var.

```toml
project      = "app-production"
api_url      = "https://api.smith.langchain.com"
repo_dir     = "/Users/cyrille/Documents/enroll/app"
fix_command  = "claude"            # split with shell-words; the prompt is appended as the last argv
since        = "7d"
org_id       = "398a5582-4144-4897-ba96-149d296b0ba7"
web_url_template = "https://smith.langchain.com/o/{org_id}/projects/p/{project_id}/r/{trace_id}"
theme        = "default"           # vtui Theme::named
prompt_template  = """..."""       # optional, default below
```

`fix_command` runs through `std::process::Command` with `current_dir(repo_dir)`, no shell. The prompt is the final positional argument: `claude --help` gives `claude [options] [prompt]` and "starts an interactive session by default, use -p/--print" for one-shot, so no `-p`. Notes are never interpolated into a shell string. The child inherits the environment unchanged.
Paths come from `$HOME` directly (no `dirs` crate, whose macOS dirs differ): config `~/.config/traces/config.toml`, marks `$XDG_DATA_HOME` or `~/.local/share/traces/marks.json`, log `~/.local/state/traces/traces.log`. Written atomically (tempfile in the same dir, then rename). Schema: `{ "version": 1, "marks": [Mark] }`, one mark per `(project, thread_id)`.
Logs: `tracing` to the log file only, never to the terminal, never message text, API response bodies or the key.

## Prompt template

Default, rendered by `prompt::render`; placeholders `{count}`, `{project}`, `{threads}`. One `{threads}` line per mark: `- {url} (thread {thread_id}, marked {marked_at}): {note}`.

```
Fix the agent behaviour problems in these {count} conversations from the LangSmith project {project}.
Each line is a thread: read its turns through the link, or with the LangSmith tools if they are available (thread id and project above), find the cause in this repository, and fix it.
The note says what is wrong. The traces hold real employee data: do not paste their content into code, tests, commits or comments.

{threads}
```

- A note with newlines is joined with spaces. An empty note renders as `(no note)`.
- Unknown placeholders are left as is and `fix` prints a warning (so a typo is visible, not silent).
- Only `Todo` marks go into the prompt. `traces fix --all` includes `Sent` ones.

## Key bindings

| Mode | Key | Action |
|---|---|---|
| list | `j`/`k`, down/up | move selection |
| list | `g`/`G` | first / last (last loads more pages) |
| list | `enter` | open thread detail |
| list | `m` | mark or edit the note of the selected thread (opens note popup) |
| list | `u` | unmark (asks `y/n`) |
| list | `f` | filter: all / marked / todo-only (cycles) |
| list | `/` | search the loaded first messages (substring, case-insensitive) |
| list | `r` | reload from the first page |
| list | `F` | fix: confirm popup listing the count, then launch |
| list | `o` | open the thread URL in the browser (`open` on macOS, `xdg-open` elsewhere) |
| detail | `j`/`k`, `ctrl-d`/`ctrl-u`, `g`/`G` | scroll turns |
| detail | `n`/`p` | next / previous turn |
| detail | `m`, `o`, `esc`/`q` | mark, open URL, back |
| note | `enter` / `esc` | save / cancel; `Field` editing keys (left, right, backspace, ctrl-w, ctrl-u) |
| any | `?` | help overlay (`help::draw`), `?` again toggles every key |
| any | `q`, `ctrl-c` | quit (confirm only when the note popup holds unsaved text) |

`keys.rs` holds one table of `(mode, KeyEvent, Msg, help text)`; the help sections are generated from it so they cannot drift.

## Screens

- List: columns `time (local, "2h ago" under 24h else date)`, first employee message (rest of width), `turns`, markers `E` errored (danger), `F` feedback (warn), mark indicator `*` todo (accent) or `v` sent (faded). Header shows project, loaded count, filter, `more` when `next_cursor` is set. Footer is the status line and the key hints.
- Detail: title `thread <short id>  <n> turns`; per turn a block `time  route  status` then `employee:` input and `agent:` reply, word-wrapped (`text::truncate` for single lines, `ratatui::Paragraph` wrap for bodies), errors in `danger`. Scroll is a line offset into the pre-rendered lines.
- Loading and error states render in the status line; fetches never block the UI.

## Fix flow

1. `F` collects `Todo` marks for the current project (from the full store, not only loaded rows) and shows a confirm popup `Launch claude on N threads in <repo_dir>? y/n`.
2. On `y`, `fix::plan` builds `FixPlan { argv, cwd, prompt }`. It fails early if `repo_dir` is not a directory or the program is not found on `PATH`.
3. The loop drops its `EventStream` (or a background reader would steal `claude`'s keystrokes), calls `block_in_place(|| screen.lend(&mut terminal, || fix::launch(&plan)))`, then builds a new `EventStream`. `claude` runs interactive in `repo_dir` and the TUI repaints on return.
4. Exit code 0: marks flip to `Sent` with `sent_at = now` and the store is saved. Non-zero or spawn error: marks stay `Todo` and the status line shows the exit code.
5. `traces fix` does the same without the TUI (`--dry-run` prints the prompt and the argv; `--yes` skips confirm), and `traces marks` prints the table (`--json`, `--status todo|sent`, `--project`).

## Error and empty states

All shown in the status line or the list body, never as a panic; the TUI starts in every case.

| Case | Shown |
|---|---|
| `LANGSMITH_API_KEY` unset or empty | list body: "LANGSMITH_API_KEY is not set"; marks and `fix` still work |
| 401 / 403 | "key rejected by LangSmith" |
| project name not found | "project <name> not found" |
| no thread in the window | "no threads since 7d, try --since 30d" |
| timeout, DNS, connection refused | "network error, press r"; loaded rows stay |
| 429, 5xx, 501/404 on v2 | typed message with the status; loaded rows stay |
| a page fails after the first | status line error, earlier pages kept |
| marks file corrupt | start-up error naming the path, file untouched |

## Event loop

`tokio::select!` over crossterm `EventStream`, an `mpsc::UnboundedReceiver<Msg>` fed by fetch tasks, and a 250 ms tick only while a fetch is in flight (spinner). Panic or `Err` restores the terminal (vtui hook, `leave`). `update(app, msg) -> (app, effects)` is pure; effects are `LoadThreads{cursor}`, `LoadTurns{thread}`, `SaveMarks`, `Fix(plan)`, `OpenUrl`, `Quit`. Each fetch carries a request id so a stale response (after `r` or a project change) is dropped. Mouse is off (`enter_without_mouse`), so terminal selection keeps working.

## Tests (no network)

| Area | Tests |
|---|---|
| `langsmith` (wiremock, inline fabricated fixtures, never real traces) | `resolve_project` picks the exact name among substring matches and errors on none |
| | `list_threads` sends `x-api-key`, `project_id`, `page_size` and an explicit `min_start_time` (body matcher) |
| | `list_threads` returns `next_cursor` and a follow-up call sends it as `cursor` |
| | `list_turns` sends each `selects` value as a repeated query param, follows `next_cursor`, stops at the page cap |
| | 401 maps to the "key rejected" error; 429, 500 and 501 become typed errors; a non-https non-localhost `api_url` is refused |
| | `read_run_url` sends `project_id` and `trace_id` and returns `url` |
| `turn.rs` | `answer`/`clarify`/`close`/`reply` read `content`; `escalate` hides `opsContent`; `confirm`; `failed`; run error; running (null `end_time`); missing `inputs.input`; unknown shape falls back to compact JSON |
| `marks.rs` | upsert replaces the note for the same `(project, thread)`; remove; `mark_sent`; atomic save round trips in a tempdir; a corrupt file is an error naming the path and is not overwritten; `version` mismatch rejected |
| `prompt.rs` | renders count, project, one line per todo mark with URL and note; newline in a note; empty note; unknown placeholder warns; `Sent` marks excluded unless `--all` |
| `config.rs` | defaults, `$TRACES_CONFIG` override, unknown key rejected, key material in the file rejected, `--since` parser (`5m`, `12h`, `7d`, garbage) |
| `fix.rs` | `plan` builds argv from `fix_command` with the prompt last, sets `cwd`; a note with quotes, `$(x)` and newlines arrives as one intact argv element; missing `repo_dir` fails; `launch` with `true` / `false` as `fix_command` returns the right status (no `claude` needed) |
| `app.rs` | pure `update` tests: no-key empty state, error states keep loaded rows, mark flow, filter cycle, selection clamp, paging effect near the end, stale response dropped, quit-confirm guard |
| `view` | a ratatui `TestBackend` smoke test per screen (renders without panic, contains the marker characters); no snapshot crate in v1 |
| `commands.rs` | `marks --json` output and `fix --dry-run` against a tempdir store |

## Build steps

| # | Step | Done when |
|---|---|---|
| 1 | Scaffold: `Cargo.toml`, lints, `rustfmt.toml`, `rust-toolchain.toml`, `clippy.toml`, `main.rs`, `cli.rs`, empty modules | `cargo build` and `cargo clippy --all-targets -- -D warnings` pass; `traces --help` lists flags and subcommands |
| 2 | `config.rs`, `ctx.rs` with tests | config tests green; a missing `LANGSMITH_API_KEY` is a typed error raised only when an API call is made |
| 3 | `model.rs`, `langsmith/wire.rs`, `turn.rs` with tests | `turn` tests green on fixture JSON built inline |
| 4 | `langsmith/mod.rs` client: `resolve_project`, `list_threads`, `list_turns` | all wiremock tests green |
| 5 | `marks.rs` store | marks tests green |
| 6 | `prompt.rs`, `fix.rs`, `commands.rs` (`marks`, `fix --dry-run`) | `traces marks --json` and `traces fix --dry-run` work on a fixture store |
| 7 | `app.rs` pure state, `keys.rs`, tests | `app` tests green, help sections generated |
| 8 | `event_loop.rs`, `view/list.rs`, status line and the error states | TestBackend smoke tests green; the user runs `traces` against the real project and sees threads, a bad key and an empty window show their messages (manual) |
| 9 | `view/detail.rs`, `view/note.rs`, help overlay, `o` key | smoke tests green; marking a thread writes `marks.json`; one opened link resolves in the browser, else fix `web_url_template` (manual) |
| 10 | Fix action through `Screen::lend` | with `fix_command = "true"` marks flip to `Sent`; with `false` they stay `Todo`; with `claude` the user sees the seeded session and typing works in it (manual) |
| 11 | README (usage, config, privacy note), `AGENTS.md` (copy the revu-tui ground rules), `ARCHITECTURE.md` module map | files exist; README says the key is env-only, what is stored, and that a note can hold anything you type |

Size guide: langsmith 350, model and turn 250, marks 250, config and ctx 200, prompt, fix and commands 350, app and keys 450, view 400, loop 150, tests inside each, about 2400. Cut first if over: `traces marks`/`url` subcommands, filter cycle, `ctrl-d/u` scroll.

Steps 3 to 6 are independent of the UI and can be reviewed as one MR each; 7 to 10 are the TUI MR.

## Out of scope for v1

- Reading `agent.reason` child runs, tool calls, or token costs.
- Writing back to LangSmith (feedback, tags) or any Notion/GitLab integration.
- Keychain or command-based key source (env var only; add `api_key_command` later).
- `$EDITOR` for long notes (`vtui::editor::edit` through `lend` is the path when wanted).
- Server-side search (`ai_search`, `sem_filter`) and date pickers.
- A cache of fetched threads on disk (the traces hold employee data; v1 keeps them in memory only).

## Decisions and what stays unverified

| Question | Answer |
|---|---|
| Reply field | `outputs.content`; see Grounding for escalate and confirm |
| Feedback fields | `n`, `avg`, `values`, no positive/negative; v1 shows presence only |
| v2 threads endpoints | both in the spec with the parameters used; 501 only on self-hosted below v0.16 |
| Project match | `name=` with `limit=100`, exact match client-side |
| Launch form | `claude <prompt>` positional, interactive; confirmed by `claude --help` |
| New activity after marking | no reset in v1: a mark stays as it is |
| `--since` | default 7 days, a window on thread activity, not creation |
| Repo | private GitHub `dcyriller/traces-tui` |
| Web URL | default template UNVERIFIED, org id configurable; `traces url` and step 9 check it |
