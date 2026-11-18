# Working in this repo

`traces` is a Rust TUI and CLI for triaging LangSmith agent traces and handing marked threads to Claude Code.
Read [`ARCHITECTURE.md`](ARCHITECTURE.md) first: the module map, the data flow and the boundaries.
The code is the contract; `ARCHITECTURE.md` describes it.
When a change moves a boundary or a flow, it updates `ARCHITECTURE.md` too.
[`docs/plan.md`](docs/plan.md) is the v1 plan this code was built from.

## Ground rules

- One MR per change a reviewer can read in one sitting, under 500 changed lines excluding fixtures.
- Branches: `<scope>/<kebab-title>` with scope in `feat fix chore refactor docs test ci`.
- Commits follow conventional commits; body in two short imperative sentences at most.
- Markdown files put one sentence per line.

## Code

- Rust 2024, `rustfmt.toml` as committed (140 columns, `use_small_heuristics = "Max"`).
- Lints live in `[lints]` in `Cargo.toml`: `cargo clippy --all-targets -- -D warnings` must be clean, and `unwrap`/`expect` only live in tests.
- Doc comments (`///`, `//!`) say what an item is for; they describe the item they sit on and stay true when it changes.
- No comment restates the code; a `//` comment says why, and only when no name can.
- Immutability across function boundaries: pass helpers values or `&` borrows and take back a new value; only the owner of a state changes it, through `&mut self`.
- Side effects at the edges only, as `ARCHITECTURE.md` § Boundaries lists them; `app.rs` and `view/` stay pure.
- Flat bodies of named steps at one level of abstraction.
- Design a signature from the call site: a caller never needs to read the body to use it right.
- Only `src/langsmith/` sees a LangSmith wire shape; everything else uses `src/model.rs`.

## Rust design rules

- Domain values get newtypes, not bare `String` (thread ids, project ids).
- Visibility is the smallest that compiles, and `unreachable_pub` enforces it.
- Never drop a `Result` silently: `drop(...)` or `.ok()` on a fallible call needs a reason a reader can see, and best-effort work logs the failure with `tracing::warn!`.
- `anyhow` at the edges; a typed error where a caller branches on the kind (`ApiError`).
- No tuple for data that travels between modules: a struct with named fields says what each part is.
- No shared mutable state across tasks when message passing works: the loop and its channel are the model.
- Never block the runtime while the TUI loop runs; the fix launch is the one deliberate exception and runs inside `block_in_place`.
- Tests are deterministic: no real network (wiremock only), and no real clock.
- One behaviour per test, named as a sentence that says it.
- One crate per job: prefer std or a dependency already in `Cargo.toml`.
- About 800 lines is the guideline for a file; a file grown past it is split when next touched.
- A module with children lives in `foo/mod.rs`, never `foo.rs` beside a `foo/` folder.

## Tests

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Every module ships its tests in the same file; view tests render on `TestBackend`.
Fixtures are invented, never real trace data.

## Security

- The key is read from `LANGSMITH_API_KEY` only: never in the config file, a log, a fixture or `Debug` output.
- The key travels to `api_url` only, and `api_url` must be `https` (or `http` on localhost, for tests).
- Traces hold real employee data: never write message text to disk, to a log, to the marks file or to the fix prompt.
- A note is typed by the user and is stored as typed.
- The repo may go public: no real thread ids, trace ids or message text in tests, docs or commits.
