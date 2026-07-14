# provider-codex

Reports real rate-limit usage for the **OpenAI Codex CLI** (`codex`).

## Detection

`codex` CLI on `$PATH`.

## Data sources, by confidence

| Confidence | Source | Notes |
|---|---|---|
| ★★★☆☆ CLI log | `~/.codex/sessions/**/*.jsonl` | Codex CLI logs a `token_count` event with a `rate_limits` object (real, server-computed `used_percent`/`window_minutes`/`resets_at`) every time it checks in with OpenAI. Never reads `~/.codex/auth.json` or any other credential file (see [SECURITY.md](../../../SECURITY.md)). |
| ★★★☆☆ CLI log | `codex login status` (stdout/stderr contains "Logged in") | Fallback connectivity-only check, used only when no session log has an account-wide rate-limit reading yet (e.g. right after install, before Codex has been used once). Never reads a credential file either. |

Unlike Claude's token-count summation (no official cap exists, so it
reports raw counts with `limit: None`), Codex's `rate_limits.used_percent`
*is* the real percentage OpenAI itself computed — so this plugin sets
`percent_used` directly rather than estimating anything, and includes a
real `resets_at` from the log's own `resets_at` unix timestamp.

### Picking the right reading — three real bugs found here

A real account's session logs turned out to carry `token_count` events
for **more than one `limit_id`** — this isn't documented anywhere, it was
found by comparing this plugin's output against ChatGPT's own "usage
exhausted" message live. On one dev machine: the account-wide `"codex"`
bucket, plus `"codex_bengalfox"` (a specific experimental model with its
own separate quota), plus a rare `"premium"` bucket with no window data at
all. Getting "the current reading" right took three separate fixes, in
this order:

1. **Filter to the account-wide bucket.** `latest_rate_limits()` now only
   considers a `token_count` line a candidate if its `limit_id` is
   `"codex"` (`ACCOUNT_WIDE_LIMIT_ID` in `src/lib.rs`) — a model-specific
   sub-quota can no longer masquerade as the account's overall rate limit.
2. **Say so when only another bucket is found.** If every reading found
   belongs to some other `limit_id`, `detail` says exactly that ("found
   rate-limit data for a different quota bucket … not the account-wide
   'codex' bucket") instead of a generic "no reading found yet" — those are
   different situations, and worth telling apart if a future plan ever
   renames or drops the `"codex"` bucket.
3. **Pick by reading timestamp, not file mtime.** Even after (1), the
   reading with the latest embedded timestamp isn't necessarily in the
   file with the latest *mtime* — a file's mtime bumps on any append,
   including a different limit_id's line, not only a fresh account-wide
   check-in. Confirmed live: two rollout files had mtimes ~20 seconds
   apart while their last `"codex"` readings were over a day apart.
   `latest_rate_limits()` now scans the same handful of recently-touched
   candidate files but keeps whichever reading's own timestamp is latest.

See the `refresh_skips_a_more_recently_modified_file_reporting_a_different_limit_id`,
`refresh_names_the_other_bucket_when_no_account_wide_reading_exists_at_all`,
and `refresh_prefers_the_reading_with_the_latest_timestamp_over_the_freshest_file_mtime`
tests for exact reproductions of each.

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
