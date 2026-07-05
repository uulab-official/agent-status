# CLAUDE.md

Guidance for Claude Code (and other Claude-based coding agents) working in
this repository. Read this before making changes — it front-loads the
context you'd otherwise need to rediscover by exploring the codebase.

## What this project is

A Rust/Tauri 2 monorepo (Cargo workspace) for a local-only (no backend) menu
bar / tray app that reports AI provider usage/limits/cost through a standard
plugin model. Read [docs/architecture.md](docs/architecture.md) before
touching more than one crate — the crate boundaries are deliberate (see "Why
`crates/plugins-common` doesn't depend on the provider crates" for the one
dependency-direction rule that's easy to violate by accident).

**This was originally a TypeScript/Electron prototype, rewritten to
Rust/Tauri before the first commit.** If you find yourself thinking "wouldn't
this be simpler with Node/npm/Electron," the answer was deliberately no —
see [docs/architecture.md](docs/architecture.md#why-rust--tauri-not-electron)
for why, and don't reintroduce that stack.

**Current state**: early scaffold. `crates/core`, `BasePluginState`, and the
Ollama + Custom + OpenRouter providers are fully implemented and tested.
Most other providers have `detect()` working and `fetch_status()` as a
documented TODO. Trust [ROADMAP.md](ROADMAP.md) over any aspirational
language elsewhere for what's actually done.

## Commands

```bash
cargo build --workspace       # build everything
cargo test --workspace        # must pass before any PR
cargo test -p <crate-name>    # scope to one crate, e.g. agent-core, provider-ollama
cargo run -p agent-status     # build and launch the actual app
```

No Node/npm/pnpm anywhere in this codebase — the popover frontend
(`ui/index.html`, `ui/popover.js`, `ui/popover.css`) is static, no build
step, no bundler. **That doesn't mean editing it is free of a rebuild,
though**: `tauri.conf.json`'s `frontendDist: "../ui"` gets embedded into
the compiled binary by `tauri-build`'s build script, so a plain `cargo run`
against an already-built binary keeps serving whatever `ui/` snapshot was
captured at the *last* `cargo build` — editing `ui/popover.js` and just
restarting the existing binary silently serves stale JS with no error. A
real bug hit while adding the popover's usage sparklines: several rounds of
"why isn't my JS change showing up" turned out to be exactly this, not a
WebView cache issue (which was also tried and didn't help, since the
binary itself hadn't changed). Always `cargo build -p agent-status` after
touching anything under `ui/`, same as after touching `src-tauri/src/`.

## Where things live (don't guess — check this first)

| If you're... | Look at |
|---|---|
| Changing what a `ProviderStatus` looks like | `crates/core/src/types.rs` — then check every provider under `crates/providers/*` for breakage |
| Adding/fixing a provider | [docs/plugin-development.md](docs/plugin-development.md), then `crates/providers/ollama/src/lib.rs` as the reference implementation |
| Touching notification thresholds | `crates/notifications/src/engine.rs` |
| Touching the tray label format | `crates/tray-label/src/label.rs` — note the fixed test expectation `"🤖 C82 G41 O99"` in its test module, which mirrors the exact mock in the root README |
| Wiring a new provider into the app | `src-tauri/src/builtins.rs` |
| Changing the popover UI | `ui/popover.js`/`ui/popover.css`/`ui/index.html` (static, no build) + `src-tauri/src/view_model.rs` (the Rust side that produces the JSON it renders) |
| Changing the SQLite schema | `crates/database/src/schema.sql` — bump `SCHEMA_VERSION` in `db.rs` if you add tables/columns that need a migration path |
| Reading/writing usage or cost history | `crates/database/src/history.rs` (`record_usage`/`record_cost`/`recent_usage`/`recent_cost`) — called from `src-tauri/src/history.rs`'s `persist()` on every scheduler tick |
| Adding/changing a `#[tauri::command]` | `src-tauri/src/commands.rs`, then register it in the `invoke_handler!` list in `src-tauri/src/lib.rs`, then call it from `ui/popover.js` via `invoke(...)` |

## Conventions specific to this repo

- **Every crate under `crates/*` avoids depending on `tauri`.** If you're
  adding logic to a `crates/*` crate and find yourself wanting
  `use tauri::...`, stop — that logic belongs in `src-tauri`.
- **`Confidence` is mandatory, not optional**, on every `LimitWindow` and
  `CostSnapshot` (they're plain fields, not `Option<Confidence>`). Never
  default to a high tier to make a plugin "look done." See
  [docs/confidence.md](docs/confidence.md).
- **Provider structs compose `BasePluginState`, they don't inherit from
  anything** (Rust has no class inheritance). Only implement `detect()` and
  `fetch_status()`/`refresh()` — see
  [docs/plugin-development.md](docs/plugin-development.md) for the exact
  pattern every existing provider follows.
- **Provider crates never import each other or `src-tauri`.** They may only
  depend on `agent-core` and `agent-plugins` (plus `reqwest`/`serde` etc. as
  needed for their own IO).
- **No comments explaining *what*.** This codebase's existing comments are
  all *why* (a workaround, a cross-reference, a non-obvious constraint, or a
  documented investigation result like the Copilot 404 in
  `crates/providers/copilot/src/lib.rs`) — match that; don't add
  narrational comments restating the code.
- **Tests that mutate process-wide env vars must serialize** with a
  `static ENV_LOCK: Mutex<()>` (see `crates/providers/openai/src/lib.rs`) —
  `cargo test` runs tests in parallel by default, and unsynchronized env-var
  mutation across tests is a real, easy-to-hit source of flakiness. Prefer
  injecting config via a constructor parameter over an env var wherever a
  test needs isolation (see `OllamaPlugin::with_base_url`).
- **HTTP-calling providers mock with `wiremock`**, not real network calls —
  see `crates/providers/ollama/src/lib.rs` or
  `crates/providers/openrouter/src/lib.rs` for the pattern.

## `ActivationPolicy::Accessory` is required, not optional

`src-tauri/src/lib.rs`'s `setup()` calls
`app.set_activation_policy(tauri::ActivationPolicy::Accessory)` before
creating any window. Don't remove this — without it, the app runs under
macOS's default `Regular` activation policy (Dock icon, regular-app focus
semantics), and the popover window would show for a single frame and
immediately hide itself again. That was a real bug found while testing
manually: macOS wasn't granting the borderless, `skip_taskbar` popover real
key-window status under `Regular`, so it fired `Focused(true)` then almost
immediately `Focused(false)`, tripping the (correct) blur-hide handler in
`lib.rs`. `Accessory` is the standard policy for menu-bar-only utilities
(no Dock icon, no Cmd+Tab entry) and fixes the focus behavior too.

## Known environment quirk: macOS menu bar auto-hide + synthetic clicks

If you're manually testing the tray icon (e.g. via a screenshot + click
loop) and a click doesn't seem to register:
1. Check whether **"Automatically hide and show the menu bar"** is enabled
   on the test machine — if so, the menu bar (and the tray icon) is only
   rendered while the mouse is at the very top edge of the screen. Move the
   cursor there first and wait briefly before clicking — a single move can
   miss; two moves a pixel apart with a short wait between reliably
   triggers the reveal.
2. Accessibility-API-driven clicks (`osascript ... click menu bar item`)
   reliably worked for the old Electron prototype's tray icon but do **not**
   reliably trigger Tauri's tray click handler — `tray-icon`'s macOS backend
   listens for real `mouseDown:`/`mouseUp:` events on the status item's
   view, not an accessibility action. Use a tool that sends genuine
   synthetic mouse events (e.g. `cliclick` on macOS) instead, and do the
   move + wait + click as a single command invocation — separate tool calls
   are often slow enough that the auto-hidden menu bar re-hides in between.

This is a real thing you will hit if you try to verify tray behavior
end-to-end; it's not a bug in this codebase.

## When you're done

Run `cargo test --workspace` before considering a change complete. If you
touched a provider's confidence tier or scope, update its README's table
and, if scope moved between roadmap milestones, update
[ROADMAP.md](ROADMAP.md) too — don't let it drift from reality.
