# src-tauri

The Tauri application. This is the only crate that:

- Depends on `tauri` itself
- Composes the built-in provider plugins (`src/builtins.rs`)
- Owns the SQLite database file location and lifecycle
- Owns the tray icon, the popover window, and the polling scheduler

Everything else — the standard model, plugin contract, notification
thresholds, tray label formatting — lives in `crates/*` precisely so it can
be unit-tested without Tauri or any GUI. See
[../docs/architecture.md](../docs/architecture.md).

## Structure

```
src/
├── lib.rs                 # tauri::Builder setup: tray icon, popover window, plugin registration
├── builtins.rs             # composition root: which provider crates get auto-detected
├── state.rs                # AppState + the shared Arc<Mutex<..>> everything locks
├── commands.rs             # #[tauri::command] handlers + the render() function they all call
├── scheduler.rs            # one tokio task per registered plugin, on its own refresh interval
├── history.rs              # persists each refresh's LimitWindow/CostSnapshot into SQLite (unit-tested)
├── view_model.rs           # ProviderStatus -> pre-formatted PopoverViewModel (unit-tested)
└── notification_bridge.rs  # AgentNotification -> native notification title/body (unit-tested)
icons/
├── icon.png                # app icon (bundle icon)
└── trayTemplate.png        # macOS template image for the tray (adapts to light/dark menu bars)
```

The frontend lives in [`../ui`](../ui) — static HTML/CSS/vanilla JS, no
build step. It talks to this crate only through
`window.__TAURI__.core.invoke()` (commands) and `.event.listen()` (the
`status-update` push).

## Status

The tray icon, popover, and settings are wired up and **have been run end to
end**: the app launches, shows a real tray icon, auto-detects whatever
providers are actually present on the machine, and clicking the tray icon
opens a popover showing each provider's state, limit bars, and reset
countdown — correctly showing `Unknown` with a TODO `detail` message for
unimplemented providers and **live data for Ollama** (verified against a
real locally-running model: `🟡 BUSY — Running: qwen2.5:0.5b (1.7 GB
VRAM)`). The popover's settings row (menu bar mode, Launch at Login) is real
too: both persist to SQLite and Launch at Login actually registers a real OS
login item via `tauri-plugin-autostart` — verified directly (checked
macOS's Login Items list before/after toggling, and queried the SQLite file
directly after a restart), not just via in-app state.

One thing intentionally still a TODO, tracked on the
[roadmap](../ROADMAP.md):

- **`fetch_status()` bodies** — most provider crates under
  `crates/providers/*` implement `detect()` for real but leave
  `fetch_status()` as a documented TODO (Ollama, the generic custom-endpoint
  plugin, and OpenRouter are the exceptions — read those first). The popover
  already renders whatever they return, so finishing a provider's
  `fetch_status()` is the only step left to see real usage bars for it.

## Running locally

```bash
cargo build -p agent-status
./target/debug/agent-status
```

No Node, no npm, no separate native-module rebuild step — `agent-database`
statically links SQLite via `rusqlite`'s `bundled` feature, so there's no
ABI-mismatch class of problem here the way the Electron prototype had with
`better-sqlite3` (see [CLAUDE.md](../CLAUDE.md) if you're curious what that
looked like — it doesn't apply to this codebase anymore, kept there as
institutional memory about *why* Rust/Tauri was chosen).

### Real bugs we found testing this manually

**Missing macOS activation policy (fixed).** The popover would show for a
frame and then immediately hide itself again — looked exactly like the
blur-hide handler misfiring. Root cause: `App::set_activation_policy` was
never called, so the app ran under the default `Regular` policy (Dock icon,
regular-app focus semantics). A borderless, `skip_taskbar` utility window
doesn't reliably get real key-window status under `Regular` — macOS handed
it a `Focused(true)` and then almost immediately a `Focused(false)`, and our
own (correct) "hide on blur" handler acted on that. Fixed in `lib.rs`'s
`setup()` by setting `tauri::ActivationPolicy::Accessory` before creating
any window — the standard policy for menu-bar-only utilities (no Dock icon,
no Cmd+Tab entry), and it also made focus handling reliable. If you're
building a similar tray-only Tauri app and see a window flash and vanish,
check this first.

**Testing quirk, not a code bug.** If your Mac has **"Automatically hide and
show the menu bar"** enabled (System Settings → Control Center), the menu
bar — and this app's tray icon — is only rendered while the mouse is at the
very top edge of the screen. Accessibility-API-driven clicks (e.g.
`osascript ... click menu bar item`) also don't reliably trigger Tauri's
tray click handler, because `tray-icon`'s macOS backend listens for real
`mouseDown:`/`mouseUp:` events on the status item's view, not an
accessibility action — unlike Electron's status item, which responds to
either. If you're scripting interaction with the tray for testing, move the
real cursor to the screen's top edge first (a plain single move can miss —
two moves a pixel apart with a short wait between reliably triggers the
reveal) and use a tool that sends genuine synthetic mouse events (e.g.
`cliclick` on macOS), not just the Accessibility API.

## Packaging

Works today for local/dev purposes — no extra config beyond what's already
in `tauri.conf.json`:

```bash
cargo install tauri-cli --version "^2.0.0" --locked   # one-time
cargo tauri build
```

Produces `target/release/bundle/macos/Agent Status.app` and a `.dmg` in
`target/release/bundle/dmg/`. This is real, not just a `cargo build` binary —
launched the actual bundled `.app` and confirmed the tray icon, popover, and
[`ActivationPolicy::Accessory`](#activationpolicyaccessory-is-required-not-optional)
(no Dock icon) all work identically to running the dev binary directly.

**Not yet done**: the bundle is only ad-hoc signed
(`codesign -dv "target/release/bundle/macos/Agent Status.app"` shows
`flags=0x20002(adhoc,linker-signed)`, no `TeamIdentifier`). Real
distribution needs a Developer ID certificate and notarization — outside
what's possible without an actual Apple Developer account. Re-verify the
notification-permission behavior noted above once a properly signed build
exists; ad-hoc signing may behave differently from a notarized build around
that OS permission prompt.
