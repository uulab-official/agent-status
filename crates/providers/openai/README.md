# provider-openai

Reports usage for **OpenAI** — currently the platform API's cost (via the
Admin Costs API). ChatGPT app/web message caps are a separate, unimplemented
data source — see below.

## Detection

`OPENAI_ADMIN_KEY` environment variable set — **not** the regular
`OPENAI_API_KEY` used for completions. The Admin Costs/Usage API
(`/v1/organization/*`) requires a key created specifically as an
["Admin API key"](https://platform.openai.com/settings/organization/admin-keys);
a regular project API key gets a 401. This is a real, deliberate distinction
in OpenAI's own API, not a naming choice made here — using the wrong key
looks like "not detected" (no admin key set) or a clear `401` error
(wrong-type key set), never a silent wrong reading.

## Data sources, by confidence

| Confidence | Source | Notes |
|---|---|---|
| ★★★★★ Official API | `GET /v1/organization/costs` (Admin API) | Real $ spend, summed from daily cost buckets for "today" and "this month". Requires `OPENAI_ADMIN_KEY`. |
| ★★☆☆☆ Browser | chatgpt.com `/settings` usage panel | Only path for ChatGPT Plus/Pro message-cap tracking; no public API for consumer plan limits. Not implemented — this plugin currently only covers the platform API spend case. |

There's no organization-level numeric spend *cap* exposed by this API (hard
limits are configured in billing settings, not queryable here), so this
plugin reports `cost` only and no `LimitWindow` — see
[docs/confidence.md](../../../docs/confidence.md) on not inventing a
percentage without a real ceiling to divide by.

## Limit windows reported

None currently. `cost.today` and `cost.this_month` are populated
(`Confidence::OfficialApi`).

## Status

`detect()` and `fetch_status()` are both implemented
(`crates/providers/openai/src/lib.rs`), covering the platform API cost case.
ChatGPT plan message caps (★★☆☆☆ browser) remain a TODO.
