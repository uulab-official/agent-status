# provider-gemini

Reports usage for **Google Gemini** — the Gemini CLI and gemini.google.com.

## Detection

- Gemini CLI on `$PATH` (`gemini`)
- `GEMINI_API_KEY` / `GOOGLE_API_KEY` environment variable set

## Data sources, by confidence

| Confidence | Source | Notes |
|---|---|---|
| ★★★★★ Official API | `GET /v1beta/models` (Generative Language API) | Confirms the key is valid and lists available models. **Not a usage/quota endpoint** — Google doesn't expose one over a simple bearer API key; real quota/spend requires the Cloud Billing/Monitoring APIs behind a full OAuth or service-account flow, out of scope here (see [Gemini API rate limits docs](https://ai.google.dev/gemini-api/docs/rate-limits): "Rate limits can be viewed in Google AI Studio" — there's no documented REST endpoint for it). |
| ★★★☆☆ CLI log | Gemini CLI's `/stats` | Interactive-only slash command inside a CLI session — investigated, but the CLI wasn't available to verify a scriptable equivalent (an `auth status`-style subcommand) against in this environment. Not implemented; would need verification against a real Gemini CLI install first, per [docs/plugin-development.md](../../../docs/plugin-development.md)'s "verify before assuming it works." |
| ★★☆☆☆ Browser | gemini.google.com usage panel | Consumer plan (Gemini Advanced) daily/weekly caps. Not implemented. |

## Limit windows reported

None. This plugin currently only confirms API key validity and lists
available models (`ModelInfo`, unauthenticated-cap-agnostic) — no
`LimitWindow` or `CostSnapshot`, since there's no queryable cap or spend
figure reachable with just `GEMINI_API_KEY`/`GOOGLE_API_KEY`.

## Status

`detect()` and `fetch_status()` are both implemented for the API-key path
(`crates/providers/gemini/src/lib.rs`). The CLI-only path (no key, only
`gemini` on `$PATH`) still degrades to `Unknown` with a clear detail
message — no sanctioned CLI status command has been verified yet.
