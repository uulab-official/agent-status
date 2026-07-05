# Architecture

## The bet this project makes

Every AI provider's usage/limits surface is a moving target — endpoints get
renamed, dashboards get redesigned, rate-limit policies change without
notice. A server-side aggregator would need constant maintenance to track
all of that centrally, for every user, forever.

Instead, Agent Status runs entirely on the user's machine as **a pipeline of
small, isolated adapters**:

```
Provider  →  Plugin  →  Standard Status Model  →  Tray / Popover
```

- **Provider** — Claude, GPT, Gemini, Cursor, Copilot, Ollama, OpenRouter, or
  any self-hosted OpenAI-compatible server. Each one has its own auth model,
  its own units (messages/tokens/credits/USD), its own reset cadence.
- **Plugin** — the only code that knows a specific provider's quirks. See
  [plugin-development.md](plugin-development.md).
- **Standard Status Model** — `ProviderStatus` (`crates/core/src/types.rs`).
  Every plugin, no matter how it got its data, produces this same shape.
- **Tray / Popover** — consumes only `ProviderStatus`. It never has a single
  `if provider_id == "claude"` branch. See [data-model.md](data-model.md).

The payoff: adding support for a new AI tool is a new crate under
`crates/providers/`, not a change to the rendering, notification, or storage
layers. Losing support for an old one (a provider shuts down, changes its
policy) is deleting that one crate.

## Why Rust + Tauri, not Electron

This project's first prototype was TypeScript on Electron. It was rewritten
to Rust/Tauri before the first commit, for one concrete reason: **a menu-bar
utility that watches other programs' resource usage shouldn't itself be one
of the heaviest things running** — bundling a full Chromium + Node runtime
for a small tray icon and popover is a real irony worth avoiding when a
lighter alternative (a native webview + a compiled binary) does the same
job. The trade-off is real too — Rust has a steeper learning curve than
TypeScript, and Tauri's plugin ecosystem is smaller than Electron's — but for
this project's shape (mostly small HTTP calls and local file reads, a small
UI, long-running background process), the lighter runtime wins.

Concretely, compared to the Electron version:
- `packages/database` (`better-sqlite3`, a native Node addon needing
  ABI-matched rebuilds for Node vs. Electron — a real recurring headache in
  the prototype) became `agent-database` (`rusqlite`, statically linked via
  the `bundled` feature — no separate native build step, no ABI mismatch
  between "run the tests" and "run the app").
- Electron's `ipcMain`/`contextBridge` became Tauri's `#[tauri::command]` +
  `app.emit()` — same shape (request/response commands, push events), less
  boilerplate.
- `app.setLoginItemSettings()` became `tauri-plugin-autostart`;
  Electron's `Notification` API became `tauri-plugin-notification`.

## Package boundaries

```
src-tauri/            The Tauri application. The ONLY crate that depends on
                     `tauri` itself, composes the built-in provider list,
                     and owns the SQLite file's lifecycle and the tray/popover.
ui/                    Static popover frontend (HTML/CSS/vanilla JS) — no
                     build step, no npm dependency. Talks to src-tauri only
                     through `window.__TAURI__.core.invoke()` and `.event`.

crates/core            ProviderStatus, the ProviderPlugin trait, PluginRegistry.
                     Zero dependencies on Tauri or any specific runtime.
crates/plugins-common   BasePluginState + detection helpers (command_exists_on_path,
                     file_exists, read_json_file_if_exists) shared by every
                     provider crate.
crates/notifications    Threshold engine: ProviderStatus -> Vec<AgentNotification>.
crates/tray-label       Pure tray-label string formatting (testable without any GUI).
crates/database         SQLite schema + migration runner + typed settings (rusqlite).

crates/providers/*      One crate per provider. Each depends on `agent-core` +
                     `agent-plugins` ONLY — never on each other, never on
                     `src-tauri`.
```

### Why `crates/plugins-common` doesn't depend on the provider crates

It's tempting to put the "register every built-in provider" composition root
inside `agent-plugins`, next to `BasePluginState`. Don't — every provider
crate already depends on `agent-plugins` for that scaffolding, so a reverse
dependency back to `crates/providers/*` would be a cycle. The composition
root (`src-tauri/src/builtins.rs`) lives in the one crate that's allowed to
depend on everything: the app itself.

## Data flow at runtime

1. On launch, `src-tauri`'s `init()` calls `create_default_registry()`,
   which runs `detect()` on every built-in plugin concurrently and registers
   whichever return `true`. Most users will have most providers
   unavailable — that's expected, not an error.
2. `scheduler::start()` spawns one independent `tokio` task per registered
   plugin, each looping on that plugin's own `refresh_interval_ms` (Ollama
   polls every 15s since it's a free local call; a scraped provider might
   poll every 5 minutes to avoid hammering a login session).
3. Each tick calls `plugin.refresh()`, which every plugin's own error
   handling (via `BasePluginState::set_error`) degrades to
   `ConnectionState::Unknown` instead of panicking the poll loop.
4. The fresh `ProviderStatus` is run through
   `NotificationEngine::evaluate()`, which dedupes by `(provider, reason)`
   so a given threshold fires once per reset cycle, not once per poll.
5. `commands::render()` recomputes the popover view model
   (`view_model.rs`), updates the tray title, and pushes the fresh snapshot
   to the popover webview via `app.emit("status-update", ...)`. The popover
   also calls the `get_view_model` command directly on load/every settings
   change instead of relying solely on that push — see
   [plugin-development.md](plugin-development.md#a-real-bug-we-hit) for why.

### A known simplification: one shared lock, not one per plugin

The scheduler's tasks all share a single `Arc<tokio::sync::Mutex<AppState>>`
rather than giving each plugin its own lock. This means refreshes serialize
through that one lock rather than running with full independent concurrency
— a deliberate v1 trade-off documented in `state.rs`, acceptable for a
handful of lightweight local checks and short-timeout HTTP calls. If a
provider's `fetch_status()` ever becomes slow enough to visibly delay
others, the fix is a per-plugin lock (`Vec<Mutex<Box<dyn ProviderPlugin>>>`)
instead of one `Mutex<AppState>`.

## Confidence is a first-class field, not an afterthought

Every number in this app came from somewhere with a different reliability
level — an official metering API vs. a scraped HTML page vs. a user's manual
entry. `Confidence` (`crates/core/src/types.rs`) is attached to every
`LimitWindow` and `CostSnapshot` specifically so the UI can be honest about
that, instead of presenting a scraped guess with the same authority as an
official API response. See [confidence.md](confidence.md).
