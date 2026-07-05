# AGENTS.md

Instructions for AI coding agents (Codex, and any other agent that reads
this file) working in this repository. Claude Code should read
[CLAUDE.md](CLAUDE.md) instead — it covers the same ground with a couple of
Claude-Code-specific notes; the facts below are the shared source of truth.

## Project summary

A Rust/Tauri 2 monorepo (Cargo workspace) for **Agent Status**, a
local-only (no backend) menu bar / tray app that reports AI provider
usage/limits/cost through a standard plugin model. Read
[docs/architecture.md](docs/architecture.md) before editing more than one
crate.

**This was originally a TypeScript/Electron prototype, rewritten to
Rust/Tauri before the first commit** — specifically so this menu-bar
utility doesn't itself bundle a full Chromium+Node runtime. Don't
reintroduce Node/npm/Electron; see
[docs/architecture.md](docs/architecture.md#why-rust--tauri-not-electron)
for the reasoning.

**Current state**: early scaffold, not a finished product. `crates/core`
(the standard model), `BasePluginState`, and the Ollama + Custom +
OpenRouter providers are fully implemented and tested. Most other providers
under `crates/providers/*` have `detect()` working but `fetch_status()` left
as a documented TODO. Treat [ROADMAP.md](ROADMAP.md) as ground truth for
what's done vs. planned — ignore any more optimistic framing elsewhere.

## Setup and validation commands

```bash
cargo build --workspace
cargo test --workspace
```

Both must pass across the workspace before a change is considered done.
Scope to one crate during iteration:

```bash
cargo test -p agent-core
cargo test -p provider-ollama
cargo build -p agent-status   # the actual Tauri app binary
```

No Node/npm/pnpm anywhere in this codebase — the popover frontend
(`ui/index.html`, `ui/popover.js`, `ui/popover.css`) is static HTML/CSS/JS
with no build step or bundler.

## Repository map

```
src-tauri/            The Tauri app — the only crate depending on `tauri`;
                     composes built-in providers in src/builtins.rs
ui/                    Static popover frontend (no build step)
crates/core/            Standard status model + ProviderPlugin trait
crates/plugins-common/   BasePluginState + detection helpers shared by all providers
crates/database/         SQLite schema + settings (rusqlite, statically linked)
crates/notifications/    Threshold-based notification engine
crates/tray-label/       Pure tray-label string formatting
crates/providers/*/      One crate per AI provider (see docs/plugin-development.md)
docs/                  Architecture, data model, confidence levels, plugin guide
```

## Hard rules (violating these breaks the architecture, not just style)

1. **Dependency direction**: `crates/providers/*` may depend only on
   `agent-core` and `agent-plugins` (plus their own IO deps like `reqwest`).
   Never the reverse, and never provider-to-provider. The composition root
   that imports every provider lives in `src-tauri/src/builtins.rs`
   specifically to avoid a cycle — see
   [docs/architecture.md](docs/architecture.md#why-cratesplugins-common-doesnt-depend-on-the-provider-crates).
2. **`Confidence` is required on every `LimitWindow`/`CostSnapshot`** (plain
   fields, not `Option`). Never pick a higher tier than the actual data
   source justifies — see [docs/confidence.md](docs/confidence.md) for the
   five tiers and their definitions.
3. **`crates/*` must not depend on `tauri`.** That's what makes them
   independently unit-testable without a GUI. If a change needs Tauri, it
   belongs in `src-tauri`.
4. **New/changed provider plugins**: implement `ProviderPlugin` by composing
   a `BasePluginState` field (Rust has no class inheritance, so this isn't
   literal subclassing) — only `detect()` and `refresh()`/`fetch_status()`
   need real logic. Follow
   [docs/plugin-development.md](docs/plugin-development.md).
5. **Tests that mutate process-wide state (env vars) must serialize** with a
   `static ENV_LOCK: Mutex<()>` (see `crates/providers/openai/src/lib.rs`) —
   `cargo test` runs in parallel by default. Prefer constructor-injected
   config over env vars for anything needing test isolation.

## Style

- No comments describing *what* code does — only non-obvious *why*
  (workarounds, invariants, citations, or documented investigation results
  like the Copilot API 404 noted in `crates/providers/copilot/src/lib.rs`).
  Match the existing comment density; don't add narration.
- Don't build abstractions for a single caller. Follow existing patterns in
  sibling crates (e.g. copy `crates/providers/openrouter` for a new
  single-API-call provider) rather than inventing new structure.
- HTTP-calling providers mock with `wiremock` in tests, not real network
  calls — see `crates/providers/ollama` or `crates/providers/openrouter`.

## `ActivationPolicy::Accessory` is required, not optional

`src-tauri/src/lib.rs`'s `setup()` calls
`app.set_activation_policy(tauri::ActivationPolicy::Accessory)` before
creating any window. This isn't cosmetic — without it, a real bug
reappears: under macOS's default `Regular` activation policy, the
borderless/`skip_taskbar` popover window doesn't reliably get real
key-window status, so it fires `Focused(true)` then almost immediately
`Focused(false)`, tripping the app's own (correct) blur-hide handler — the
popover shows for a single frame and vanishes. `Accessory` is the standard
policy for menu-bar-only utilities (no Dock icon, no Cmd+Tab entry) and
fixes the focus behavior too. Don't remove this call.

## Known environment quirk: macOS menu bar auto-hide + synthetic clicks

If manually verifying the tray icon: "Automatically hide and show the menu
bar" (macOS System Settings) means the tray icon only renders while the
mouse is at the screen's top edge — a single move can miss the reveal; two
moves a pixel apart with a short wait between is more reliable. Also,
Accessibility-API clicks (`osascript ... click menu bar item`) do **not**
reliably trigger Tauri's tray click handler — it listens for genuine
`mouseDown:`/`mouseUp:` events, not an accessibility action. Use a real
synthetic-mouse-event tool (e.g. `cliclick`), and combine move + wait +
click into one invocation, since separate tool calls are often slow enough
for the menu bar to re-hide in between. This part is a real macOS/Tauri
interaction to account for when testing, not a bug in this codebase.

## Definition of done for a change here

- `cargo test --workspace` passes.
- If you changed a provider's confidence tier, data source, or scope, its
  `README.md` table reflects that.
- If scope moved between milestones, [ROADMAP.md](ROADMAP.md) is updated in
  the same change — it should never describe a state the code doesn't
  actually match.
