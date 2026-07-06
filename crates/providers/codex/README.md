# provider-codex

Reports real rate-limit usage for the **OpenAI Codex CLI** (`codex`).

## Detection

`codex` CLI on `$PATH`.

## Data sources, by confidence

| Confidence | Source | Notes |
|---|---|---|
| ★★★☆☆ CLI log | `~/.codex/sessions/**/*.jsonl` | Codex CLI logs a `token_count` event with a `rate_limits` object (real, server-computed `used_percent`/`window_minutes`/`resets_at` for a "primary" ~5-hour and "secondary" ~7-day window) every time it checks in with OpenAI. This plugin reads the *last* such event in the most recently modified session file — the same class of source as `provider-claude`'s `~/.claude/projects/**/*.jsonl` parsing. Never reads `~/.codex/auth.json` or any other credential file (see [SECURITY.md](../../../SECURITY.md)). |
| ★★★☆☆ CLI log | `codex login status` (stdout/stderr contains "Logged in") | Fallback connectivity-only check, used only when no session log has a rate-limit reading yet (e.g. right after install, before Codex has been used once). Never reads a credential file either. |

Unlike Claude's token-count summation (no official cap exists, so it
reports raw counts with `limit: None`), Codex's `rate_limits.used_percent`
*is* the real percentage OpenAI itself computed — so this plugin sets
`percent_used` directly rather than estimating anything, and includes a
real `resets_at` from the log's own `resets_at` unix timestamp.

## What it reports

- Two `LimitWindow`s when a session log has a reading: window label derived
  from `window_minutes` (300 → "5-hour", 10080 → "Weekly"), `percent_used`
  set directly from `rate_limits.primary`/`.secondary`, real `resets_at`
- `state`: `Online` whenever a rate-limit reading exists, or when
  `codex login status` reports a logged-in session with no reading yet;
  `Unknown` only when neither signal is available
- `detail` explains which of the two sources produced the current reading

## Status

Fully implemented. If OpenAI changes the session log's `rate_limits` shape,
`window_label()`'s minutes-based derivation should keep working without
changes; if the *field names* change, `parse_rate_limits()` needs updating
(it fails closed — a shape it doesn't recognize falls back to the
connectivity-only check rather than panicking or reporting stale data).
