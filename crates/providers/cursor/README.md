# provider-cursor

Reports **Cursor**'s connection status today; Premium/Slow request usage and
monthly reset once `fetch_status()` grows a real quota source (see below).

## Detection

- `~/.cursor` config directory present, OR
- `cursor-agent` CLI on `$PATH`

## Data sources, by confidence

| Confidence | Source | Notes |
|---|---|---|
| ★★★★☆ Official screen | cursor.com dashboard usage response | **Investigated and deliberately not implemented.** Getting this requires a session cookie, and the only way to obtain one is reading it out of `~/.cursor`'s stored session data directly — the same "open another tool's credential store" pattern this project rejected for Claude's Keychain-stored OAuth token (see [SECURITY.md](../../../SECURITY.md) and [ROADMAP.md](../../../ROADMAP.md)'s Claude entry). Would revisit if Cursor ships a sanctioned CLI/API path instead. |
| ★★★☆☆ CLI log | `cursor-agent status` (stdout/stderr contains "logged in") | **Implemented.** The CLI's own sanctioned way of answering "am I logged in" — this plugin never reads `~/.cursor`'s stored session data directly. Investigated `cursor-agent --help` directly: no `usage`/`limits` subcommand exists, only `status`/`whoami`. |
| ★★☆☆☆ Browser | In-editor usage indicator scrape | Not pursued — same credential-store problem as the dashboard row above (the in-editor usage indicator is only visible to an already-authenticated Cursor session). |

## What it reports today

- `state`: `Online` when `cursor-agent status` reports a logged-in session,
  `Unknown` otherwise
- No `limits` — real Premium/Slow quota numbers would need the dashboard
  endpoint above, which this plugin deliberately doesn't pursue (see table)

## Limit windows reported

None. Would be `premium_requests` (monthly Premium request allotment) and
`requests` (Slow-pool usage) if a sanctioned quota source is ever added.

## Status

Connectivity check is real and implemented (upgraded from a `TODO`-only
stub — see `crates/providers/codex` for the same pattern applied to another
CLI). Real quota numbers are blocked on Cursor shipping a sanctioned way to
read them (CLI subcommand or public API) — not on effort spent here.
