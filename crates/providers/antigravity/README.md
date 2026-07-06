# provider-antigravity

Reports quota status for **Google Antigravity** (agentic VS Code-fork IDE),
sourced from its own local, non-credential quota cache.

## Detection

`~/.antigravity` config directory present (created on first launch).

## Data sources, by confidence

| Confidence | Source | Notes |
|---|---|---|
| ★★★☆☆ CliLog | `~/.antigravity_cockpit/cache/quota/local/*.json` | A cache Antigravity's own UI maintains for itself — `updatedAt` (ms epoch) plus a `models` array, each with `displayName`/`remainingPercentage`/`resetTime`/`isRecommended`. Same class of source as `provider-codex`'s session-log rate limits: a tool's own locally-persisted reading of a real server-side quota, not fetched live and not its credential store. |
| — | none | There's still no `antigravity` CLI on `$PATH` for a sanctioned "am I logged in" check, and `~/.antigravity_cockpit/credentials.json` is a credential file this plugin will not open directly (see [SECURITY.md](../../../SECURITY.md) and the Claude/Cursor entries in [ROADMAP.md](../../../ROADMAP.md) for the same line drawn elsewhere in this codebase). |

The cache holds one entry per model — 10+ `isRecommended` models on a real
account. Rather than showing all of them (too many for a popover row) or
picking one arbitrarily, `fetch_status()` reports the single model with the
lowest `remainingPercentage` among the models Antigravity currently
recommends, labeled with that model's own display name (e.g. "Claude Sonnet
4.5") so it's clear which model the percentage refers to.

## Staleness

Antigravity's own quota resets are roughly a day apart (`resetTime` deltas
observed ~24h). A cache reading older than 24 hours is treated as too stale
to trust and reported as `Unknown` (same principle, and same bug class, as
the one found and fixed for `provider-codex`'s rate limits — see its
README/ROADMAP entry). On a machine where Antigravity hasn't been used
recently, this cache can go stale for weeks to months; that's expected, and
correctly results in Antigravity not appearing in the popover at all rather
than showing a number that's no longer true.

## What it reports

- `state`: `Online` when a fresh quota reading exists, `Unknown` otherwise
  (no cache found, or the cache is too old — see Staleness above)
- One `LimitWindow` (`antigravity:quota`) for the closest-to-limit
  recommended model, with a real `resets_at` from that model's own
  `resetTime`
- No `cost` (not exposed by this cache)
- `detail` explains the source/staleness so this doesn't read as a bug

## Deliberately not pursued

A second path — reading a CSRF token out of Antigravity's running
`language_server` process to call its loopback-only local API for *live*
per-model quota (what `wusimpl/AntigravityQuotaWatcher` and
`codavidgarcia/antigravity-pulse` do) — was implemented and then reverted.
The coding agent's own safety classifier blocked both testing and building
it, classifying process-argument token extraction as the same category as
reading a stored credential file, even though it's a different thing in
practice (an ephemeral loopback-auth token from a running process's already-
visible arguments, not an on-disk account credential). See ROADMAP.md before
re-attempting this — it needs an explicit, informed decision from a human
maintainer about accepting that classification risk, not another automated
attempt expecting a different result.
