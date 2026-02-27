# Newsagent

AI agent that generates the "Tech Updates" section of a newsletter. Uses Gemini as the LLM backend via `rig-core`, orchestrating tools to gather context from Todoist, web pages, local markdown files, and Discourse instances.

## Build & Test

```bash
cargo build                  # debug build
cargo test                   # run all tests (unit + integration)
cargo fmt --check            # check formatting
cargo clippy                 # lint
cargo run                    # run the agent (requires .env)
```

Nix dev shell is available via `nix develop` (or automatically with direnv). CI runs `cargo fmt --check`, `cargo clippy`, `cargo build`, and `cargo test` inside the Nix shell.

## Architecture

```
src/
├── main.rs              # Entry point: loads .env, parses config, runs agent
├── lib.rs               # Module declarations
├── config.rs            # AppConfig (flat env var struct via envy)
├── agent/
│   ├── mod.rs           # Agent struct wrapping rig::agent::Agent
│   └── prompt.rs        # System prompt constant + dynamic prompt builder
└── tools/
    ├── mod.rs           # Tool module declarations
    ├── todoist.rs       # TodoistTasksTool - fetches tasks from Todoist API
    ├── web.rs           # WebReadabilityTool - extracts content from URLs
    ├── glean.rs         # GleanTool - gathers local markdown as style context
    └── discourse.rs     # DiscourseTool - fetches from Discourse API (optional)
```

**Flow:** `main` → `AppConfig::from_env()` → `Agent::new(config)` (registers tools, gathers glean context) → `agent.prompt()` (builds dynamic prompt, runs multi-turn agentic loop with up to 20 turns).

### Tool pattern

Every tool implements `rig::tool::Tool` with these associated types:

- `{Name}Tool` struct — holds config + HTTP client
- `{Name}Args` — `Deserialize` input from LLM
- `{Name}Output` — `Serialize` result back to LLM
- `{Name}Error` — `thiserror::Error` enum, always has `Other(#[from] anyhow::Error)` variant

Tool names are snake_case strings: `todoist_tasks`, `browse_web`, `local_markdown_context`, `discourse_fetch`.

### Configuration

All config via `NEWSAGENT_`-prefixed env vars, parsed by `envy` into `AppConfig`. Sub-configs use `#[serde(flatten)]` so all vars share the same `NEWSAGENT_` prefix. Fields use `#[serde(rename = "tool_field")]` to map to env var names.

**Required:** `NEWSAGENT_GEMINI_API_KEY`, `NEWSAGENT_TODOIST_API_TOKEN`, `NEWSAGENT_TODOIST_PROJECT_ID`, `NEWSAGENT_GLEAN_DIR`

Optional tools (like Discourse) return `Option<Self>` from `new()` and are conditionally registered.

Custom deserializers exist for `Option<usize>`, `Option<u64>`, and comma-separated Discourse instance lists.

## Test Conventions

Tests live in `tests/` as integration tests:

```
tests/
├── common/mod.rs        # EnvGuard + with_newsagent_env() helper
├── config.rs            # Config parsing tests
├── agent/               # Agent + prompt tests
│   ├── main.rs
│   ├── agent.rs
│   └── prompt.rs
└── tools/               # Tool tests (one file per tool)
    ├── main.rs
    ├── todoist.rs
    ├── web.rs
    ├── glean.rs
    └── discourse.rs
```

**Key patterns:**

- `with_newsagent_env(vars)` returns an `EnvGuard` (RAII) that acquires a mutex, clears all `NEWSAGENT_*` vars, sets the given vars, and clears again on drop. Always assign to `_guard` to keep it alive.
- HTTP tools are tested with `wiremock::MockServer` — create a server, mount mocks, pass `server.uri()` as the tool's `base_url`.
- Filesystem tools use `tempfile::TempDir`.
- All async tests use `#[tokio::test]`.
- Tests construct tool structs directly with their `Config` types (not through `AppConfig`).

## Code Style

- Error handling: `thiserror` enums for tool errors, `anyhow::Context` for wrapping
- Minimal comments — code is self-documenting through clear naming
- No doc comments on public items (this is intentional, not something to "fix")
- `log` crate for runtime logging (`log::info!`, `log::debug!`, `log::warn!`)
- Single `reqwest::Client` per tool instance (connection pooling)
- Content truncation uses char count (`.chars().count()`) not byte length

## Plans

Implementation plans for larger features are stored in `plans/` as markdown files.
