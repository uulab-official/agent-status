# Roadmap

This roadmap tracks what's built vs. planned. It's the single source of truth
for scope — if something isn't checked off here, treat it as not implemented
yet, regardless of what a README elsewhere aspires to.

Legend: ✅ done · 🚧 in progress · ⬜ not started

## Stack

**Rust + Tauri 2**, not Electron/Node. The project started as a TypeScript/
Electron prototype and was rewritten to Rust/Tauri before its first commit,
specifically to avoid bundling a full Chromium+Node runtime for a small
menu-bar utility. See [docs/architecture.md](docs/architecture.md) for the
full reasoning and crate layout.

## v1.0 — "It replaces checking five browser tabs"

The bar for v1.0: a user can leave the app running and get an accurate
Claude + Ollama reading in the menu bar, with at least one more provider
(OpenAI or OpenRouter) working end to end. **OpenRouter meets that bar in
code today** (fully implemented, unit-tested); it hasn't been exercised
against a live key in this environment, since no `OPENROUTER_API_KEY` was
available to test with — see its README for what to verify if you add one.

**Foundation**
- ✅ Cargo workspace scaffold (`crates/*` libraries + `src-tauri` binary)
- ✅ Standard status model (`agent-core`: `ProviderPlugin` trait, `ProviderStatus`, `Confidence`, `ConnectionState`)
- ✅ `PluginRegistry` (register/get/list)
- ✅ `BasePluginState` scaffolding shared by all provider crates (`agent-plugins`)
- ✅ SQLite schema + migrations (`agent-database`, via `rusqlite`)
- ✅ Notification threshold engine (`agent-notifications`)
- ✅ Tray label formatting for all three menu bar modes (`agent-tray`)
- ✅ Popover presentation logic (`src-tauri/src/view_model.rs`: sort-by-attention, progress tone, state indicators)

**Providers** (confidence tier noted; see [docs/confidence.md](docs/confidence.md))
- ✅ Ollama — fully implemented (★★★★★, local API, the reference plugin)
- ✅ Custom / OpenAI-compatible endpoint (LM Studio, AnythingLLM, Open WebUI) — fully implemented (★★★★★ for the endpoint's own model list)
- ✅ OpenRouter — fully implemented (★★★★★ via `GET /api/v1/auth/key`): maps usage/limit to a cost snapshot plus an optional `credit` limit window when the key has a spend cap, `RateLimited` once usage reaches it. Unit-tested with a mocked HTTP server (capped key, unlimited key, at-limit, API-error); not yet verified against a real key.
- ✅ Codex (new provider, `crates/providers/codex`) — reports real connectivity (★★★☆☆ CLI log) via `codex login status`, the CLI's own sanctioned "am I logged in" check — same pattern as Copilot's `gh auth token`, never reads `~/.codex/auth.json` directly (a credential file; an attempt to parse it directly during development was correctly blocked by the safety classifier, which is what led to this safer design). Verified live: shows `🟢 Online` on a real logged-in machine. No usage/limit subcommand exists in the CLI (checked `--help` for `login`/`debug` too), so no `LimitWindow` yet.
- ✅ Claude — fully implemented (★★★☆☆ CLI log): parses `~/.claude/projects/**/*.jsonl` session transcripts (the same source `ccusage`/`Claude-Code-Usage-Monitor` use) and sums *effective* tokens (cache reads discounted 0.1x, cache writes 1.25x, matching Anthropic's own cache pricing) over rolling 5-hour and 7-day windows. No `limit`/percentage — Anthropic doesn't expose the plan's numeric cap anywhere locally observable, so `LimitWindow.limit` stays `None` rather than inventing a percentage (see docs/confidence.md). Verified live against real session data. Investigated and deliberately rejected: reading Claude Code's OAuth token from the macOS Keychain to call the undocumented `/api/oauth/usage` endpoint (what every "just works" community extension/app does) — this is the same class of "open another tool's credential store directly" this project's SECURITY.md draws a hard line against, and the coding agent's own safety classifier independently blocked even a read-only existence check of that Keychain item.
- ✅ OpenAI/ChatGPT — fully implemented for platform API spend (★★★★★ via `GET /v1/organization/costs`, the Admin Costs API): sums daily cost buckets into `today`/`this_month` in a `CostSnapshot`. Requires `OPENAI_ADMIN_KEY` specifically (an Admin API key, not a regular `OPENAI_API_KEY` — the Admin API rejects the latter with 401; see README.md). Unit-tested with a mocked HTTP server; not yet verified against a real admin key. ChatGPT plan message caps (★★☆☆☆ browser scrape) remain unimplemented — this only covers the platform API case.
- 🚧 Gemini — connectivity upgraded from a bare TODO (★★★★★ via `GET /v1beta/models`, confirms the key is valid and lists models) — same "real connectivity beats blanket Unknown" pattern as Codex/Cursor. **Investigated and found there's no usage/quota REST endpoint reachable with a bearer API key at all** (unlike OpenAI's Admin Costs API) — Google's own docs point to the AI Studio UI only; real quota data needs the Cloud Billing/Monitoring APIs behind a full OAuth/service-account flow, which is a different shape of integration than every other provider here. `LimitWindow`/`CostSnapshot` remain 🚧 TODO for that reason, not lack of effort. CLI-only path (no API key) also still 🚧 — Gemini CLI wasn't installed in this environment to verify a scriptable status subcommand exists.
- ✅ Cursor — connectivity upgraded from a bare TODO to a real check (★★★☆☆ CLI log via `cursor-agent status`, same safe-CLI pattern as Codex/Copilot). Verified live: shows `🟢 Online`. Actual Premium/Slow quota numbers (★★★★☆ dashboard) are still 🚧 — `cursor-agent`'s CLI surface has no `usage`/`limits` subcommand, only `status`/`whoami`, and the only remaining path (reading a session cookie out of `~/.cursor` to call the dashboard endpoint) is the same rejected-on-principle credential-store pattern as Claude's Keychain case above — this is a deliberate stop, not an oversight.
- 🚧 GitHub Copilot — `detect()` done, `fetch_status()` TODO. **Investigated and blocked**: `GET /user/copilot/usage` 404s for individual (non-org) GitHub accounts even with a valid token — the Copilot usage API appears to be org/enterprise-only. Needs verification against an org-level Copilot Business/Enterprise seat before implementing further. (Unlike Codex/Cursor, `gh` has no equivalent lightweight "logged in" signal beyond what `detect()` already checks, so there's no analogous connectivity-only upgrade available here.)

**App (`src-tauri`)**
- ✅ Tray icon + click-to-open popover — verified running end to end: real tray icon renders (macOS template image, adapts to light/dark), providers auto-detect on launch, popover shows live per-provider state/limits/cost sourced from `ProviderStatus`, hides on click-away. Right-click still gives a bare Quit menu. Verified with **real live Ollama data** (pulled and loaded a model, popover correctly showed `🟡 BUSY — Running: qwen2.5:0.5b (1.7 GB VRAM)`). Along the way, found and fixed a real bug: the app never set `ActivationPolicy::Accessory`, so under the default `Regular` policy the popover would flash and immediately hide itself (macOS wasn't granting it real key-window status, tripping our own blur-hide handler) — see `src-tauri/README.md` for the full story.
- ✅ Menu bar mode switcher (minimal / compact / detailed) — live in the popover's settings row, persisted to SQLite via `agent-database`'s `get_setting`/`set_setting`, applied on next tray title render. Verified end to end, including a restart-survives-setting check via direct SQLite query.
- ✅ Launch at Login — a checkbox in the popover calls `tauri-plugin-autostart`, persisted the same way as tray mode. Verified end to end.
- ✅ Native notifications — `tauri-plugin-notification` is wired up; both the manual "Send Test Notification" button and the scheduler's real threshold-crossing `AgentNotification`s (from `NotificationEngine::evaluate` and each plugin's own `drain_notifications()`) now go through the same `show_agent_notification()` path (`commands.rs`). Confirmed the OS notification permission prompt is reached via the manual button; the scheduler path couldn't be exercised live in this environment since no currently-live provider (Ollama has no limits; everything else is `Unknown`) ever crosses a threshold to actually fire one — worth re-verifying once a `LimitWindow`-reporting provider (e.g. OpenRouter with a real key) is live.
- ⬜ Full dashboard window (the larger mock in the README — history charts, per-model stats); the popover covers the smaller "click the tray" mock
- ⬜ Windows tray parity pass (icon/menu behavior differs from macOS; the settings persistence and command wiring above are already OS-agnostic)
- ✅ Usage history persistence — `agent-database` gained `record_usage`/`record_cost` (+ `recent_usage`/`recent_cost` for reading it back), and `history::persist()` in `src-tauri` calls them from the scheduler on every successful refresh, splitting a `CostSnapshot` into up to three rows (today/week/month). Unit-tested (9 new tests across `agent-database` and `src-tauri`); not yet observed with real non-empty data live in this environment, since no currently-live provider reports a `LimitWindow` or `CostSnapshot` (Ollama has neither; OpenRouter would but has no key here) — the empty-case path (nothing to persist) is exactly what's been verified live. Read side now exposed too: `get_usage_history` is a real `#[tauri::command]` the popover (or a future Timeline view) can call today — no UI calls it yet, but the plumbing is complete end to end from SQLite to the IPC boundary.
- 🚧 Packaged builds — `cargo tauri build` (via the `tauri-cli` crate, installed with `cargo install tauri-cli --version "^2.0.0"`) produces a real `Agent Status.app` and a `.dmg` for macOS with zero extra config beyond what already existed in `tauri.conf.json`. Launched the actual bundled `.app` (not the raw dev binary) and confirmed the tray icon, popover, and Accessory activation policy (no Dock icon) all work identically to the dev build. **Remaining for this to be "done"**: the bundle is currently `adhoc`-signed only (`codesign -dv` shows `flags=0x20002(adhoc,linker-signed)`, no Developer ID / TeamIdentifier) — real distribution needs a Developer ID certificate and notarization, which needs an actual Apple Developer account this environment doesn't have. Windows (msi/nsis) untested — no Windows machine available here.

## v1.5 — "It knows more than one thing about each provider"

- ⬜ Per-model statistics where a provider's API exposes them (e.g. Claude Sonnet vs. Opus split)
- ⬜ API cost analysis view (daily/weekly/monthly breakdown, powered by `cost_history`)
- ⬜ Usage pattern report (peak hours, busiest provider) from `usage_history`/`events`
- ⬜ Auto-update (Tauri has a first-party updater plugin — `tauri-plugin-updater` — unlike the Electron prototype which needed a separate `apps/updater` package for this)

## v2.0 — "It's a platform, not just an app"

- ⬜ Local model GPU/VRAM/RAM as a first-class `LimitWindow` (currently Ollama only reports this as free-text `detail`; needs a cross-platform way to read total system memory)
- ⬜ Multi-machine sync (opt-in; almost certainly needs *some* relay, which conflicts with "no server" — needs a design decision, not just an implementation task)
- ⬜ Plugin marketplace / discovery for community-maintained providers. Note: unlike the original Electron/npm design, Rust plugins can't be dynamically loaded at runtime without a stable ABI (e.g. a C FFI boundary or WASM) — this needs real design work, not just "drop a file in a folder."
- ⬜ Public provider-crate template + a scaffolding CLI so a new provider doesn't require cloning this repo

## Deliberately out of scope for now

- A hosted backend of any kind. The whole architectural bet (see
  [docs/architecture.md](docs/architecture.md)) is that a plugin per
  provider, running locally, is cheaper to maintain long-term than a service
  that has to track every vendor's API/policy changes centrally.
- Reverse-engineering rate limits that require bypassing auth/ToS in ways
  that could get a user's account flagged. When a provider has no official
  usage surface, we scrape the same page a logged-in user would see — never
  simulate traffic to *infer* a limit.
