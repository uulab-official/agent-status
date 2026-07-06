# provider-antigravity

Reports detection status for **Google Antigravity** (agentic VS Code-fork
IDE). See below for why this is a smaller step than Codex/Cursor/Copilot's
connectivity-only upgrade.

## Detection

`~/.antigravity` config directory present (created on first launch).

## Data sources, by confidence

| Confidence | Source | Notes |
|---|---|---|
| — | none available | No `antigravity` CLI exists on `$PATH` to shell out to for a sanctioned "am I logged in" check, unlike `codex login status` / `cursor-agent status` / `gh auth token`. The only auth state on disk lives in `~/.antigravity_cockpit/credentials.json` — a credential file this plugin will not open directly (see [SECURITY.md](../../../SECURITY.md) and the Claude/Cursor entries in [ROADMAP.md](../../../ROADMAP.md) for the same line drawn elsewhere in this codebase). |

Unlike Codex/Cursor/Copilot, `detect()` finding the config directory
doesn't by itself mean a session is currently logged in — there's no
sanctioned way to check that without either a CLI (which doesn't exist) or
reading the credential file (which this project won't do). So
`fetch_status()` reports `Unknown` rather than assuming `Online`, which
would be guessing.

## What it reports

- `state`: always `Unknown` today
- No `limits`, no `cost`
- `detail` explains why, so this doesn't read as a bug

## Status

Detection only. Would upgrade to a real connectivity check the moment
Antigravity ships a CLI with a login-status subcommand (same pattern as
`crates/providers/codex`) — revisit if/when that happens. Until then, this
plugin's `Unknown` state means it won't show up in the popover at all (see
`src-tauri/src/view_model.rs`'s Unknown-filter), same as GitHub Copilot's
current blocked state.
