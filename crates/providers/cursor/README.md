# provider-cursor

Reports **Cursor**'s connection status today; Premium/Slow request usage and
monthly reset once `fetch_status()` grows a real quota source (see below).

## Detection

- `~/.cursor` config directory present, OR
- `cursor-agent` CLI on `$PATH`

## Data sources, by confidence

| Confidence | Source | Notes |
|---|---|---|
| ★★★★☆ Official screen | cursor.com dashboard usage response | **Not yet implemented.** Cursor's billing dashboard returns usage as JSON; treated as "official screen" rather than a public API since it's not a documented/stable contract. |
| ★★★☆☆ CLI log | `cursor-agent status` (stdout/stderr contains "logged in") | **Implemented.** The CLI's own sanctioned way of answering "am I logged in" — this plugin never reads `~/.cursor`'s stored session data directly. Investigated `cursor-agent --help` directly: no `usage`/`limits` subcommand exists, only `status`/`whoami`. |
| ★★☆☆☆ Browser | In-editor usage indicator scrape | Fallback if the dashboard call requires interactive auth we don't have. |

## What it reports today

- `state`: `Online` when `cursor-agent status` reports a logged-in session,
  `Unknown` otherwise
- No `limits` yet — real Premium/Slow quota numbers need the dashboard
  endpoint above, which needs a session cookie this plugin doesn't have a
  safe way to obtain yet

## Limit windows reported (once the dashboard call is implemented)

- `premium_requests` — monthly Premium request allotment
- `requests` — Slow-pool usage (unlimited but queued)

## Status

Connectivity check is real and implemented (upgraded from a `TODO`-only
stub — see `crates/providers/codex` for the same pattern applied to another
CLI). The actual quota numbers are still a TODO pending the dashboard
integration.
