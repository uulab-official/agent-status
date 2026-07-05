# Contributing

Thanks for considering contributing to Agent Status. The project is early —
check [ROADMAP.md](ROADMAP.md) before starting anything nontrivial to see if
it's already scoped, and to place your work in the right milestone.

## The highest-leverage contribution right now

Most providers under `crates/providers/` have `detect()` implemented but
`fetch_status()` left as a TODO. Finishing one of those is the single most
valuable thing you can send a PR for. Read
[docs/plugin-development.md](docs/plugin-development.md) end to end first —
it walks through the exact pattern, using `crates/providers/ollama` as the
reference implementation.

## Setup

```bash
git clone <your fork>
cd agent-status
cargo build --workspace
```

Requires the Rust toolchain (install via [rustup](https://rustup.rs)). No
Node/npm is needed — the popover frontend under `ui/` is static HTML/CSS/JS
with no build step.

## Before opening a PR

```bash
cargo test --workspace
cargo build -p agent-status   # confirm the app itself still builds
```

Both must pass. If you're touching a provider, follow the checklist in
[docs/plugin-development.md](docs/plugin-development.md) — in particular:
every `LimitWindow`/`CostSnapshot` needs an honest `Confidence` (see
[docs/confidence.md](docs/confidence.md)), and the provider's README needs
to document its detection strategy and confidence tiers.

## Adding a new provider

1. Read [docs/plugin-development.md](docs/plugin-development.md).
2. Scaffold `crates/providers/<name>` following an existing crate as a
   template (`crates/providers/openrouter` for a single authenticated API
   call, `crates/providers/custom` for a config-driven multi-instance
   provider). Add it to the root `Cargo.toml`'s workspace members and
   dependencies.
3. Register it in `src-tauri/src/builtins.rs` if it auto-detects, or wire it
   into a future Settings screen if it needs required config.
4. Add it to the provider table in `README.md`/`README.ko.md` and to
   `ROADMAP.md`.

## Code conventions

- No comments explaining *what* code does — names should do that. Comments
  are for non-obvious *why* (a workaround, an invariant, a source citation
  like "per OpenRouter's docs at ...", or a documented investigation result
  like the Copilot API 404 noted in `crates/providers/copilot/src/lib.rs`).
- Don't add abstractions ahead of a second concrete use case. Three similar
  lines across two provider plugins is fine; a shared helper for one caller
  isn't.
- Every crate that can be tested without Tauri/a GUI, should be — that's the
  whole reason `crates/*` is split out from `src-tauri`. If you find
  yourself wanting to depend on `tauri` from a `crates/*` library, that
  logic probably belongs in `src-tauri` instead.
- Tests that mutate process-wide state (env vars) must serialize with a
  `static ENV_LOCK: Mutex<()>` — see `crates/providers/openai/src/lib.rs`.
  Prefer injecting config via a constructor (see `with_base_url` in
  `provider-ollama`) over env vars wherever a test needs to isolate state.

## Reporting bugs / requesting features

Use the issue templates under `.github/ISSUE_TEMPLATE/`. For a provider
returning wrong data, include which `Confidence` tier the plugin claims —
it changes whether it's a bug (wrong parsing of an official API) or expected
limitation (a scrape target changed shape).

## Security

See [SECURITY.md](SECURITY.md) — do not open a public issue for a
credential-handling or auth-bypass concern.
