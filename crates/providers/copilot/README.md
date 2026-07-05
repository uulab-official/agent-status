# provider-copilot

Reports **GitHub Copilot**'s premium-request quota and reset date.

## Detection

- `gh` CLI on `$PATH`, OR
- `GITHUB_TOKEN` / `GH_TOKEN` environment variable set

## Data sources, by confidence

| Confidence | Source | Notes |
|---|---|---|
| ★★★★★ Official API | GitHub REST `GET /user/copilot/usage` (or org equivalent) | **Investigated and blocked**: this 404s for individual (non-org) accounts even with a valid token from `gh auth token` with `repo`/`read:org`/`workflow` scopes. The Copilot usage API appears to require an org-level Copilot Business/Enterprise seat — needs verification against one before implementing further. |
| ★★★☆☆ CLI log | `gh copilot` local state | Fallback when only `gh` auth is available. |

## Limit windows reported (once implemented)

- `premium_requests` — monthly premium request allotment
- `requests` — unlimited-tier chat/completions usage (informational only)

## Status

`detect()` implemented, `fetch_status()` is a TODO and currently blocked on
API access (see above) rather than just unwritten — this is v1.5 scope on
the roadmap.
