# provider-codex

Reports connection status for the **OpenAI Codex CLI** (`codex`).

## Detection

`codex` CLI on `$PATH`.

## Data sources, by confidence

| Confidence | Source | Notes |
|---|---|---|
| ★★★☆☆ CLI log | `codex login status` (stdout contains "Logged in") | The CLI's own sanctioned way of answering "am I logged in" — this plugin never reads `~/.codex/auth.json` or any other credential file directly. Same pattern as `provider-copilot`'s `gh auth token`. |

**Investigated and found no usage/limit surface**: `codex --help`,
`codex login --help`, and `codex debug --help` were checked directly against
a real installed CLI — there is no `usage`, `status`, or `limits`
subcommand exposing rate-limit or quota data. `codex login status` only
reports whether a session is active.

## What it reports

- `state`: `Online` when `codex login status` reports a logged-in session,
  `Unknown` otherwise
- No `limits` — there is nothing to report yet; see above
- `detail` explains why, either way

## Status

Fully implemented for what's achievable today (real connectivity check, no
credential file parsing). If OpenAI ships a usage API reachable via a
sanctioned token-export mechanism (comparable to `gh auth token`), extend
`fetch_status()` to call it — don't parse `~/.codex/auth.json` directly to
get there; see SECURITY.md.
