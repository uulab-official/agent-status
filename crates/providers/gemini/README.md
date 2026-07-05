# provider-gemini

Reports usage for **Google Gemini** — the Gemini CLI and gemini.google.com.

## Detection

- Gemini CLI on `$PATH` (`gemini`)
- `GEMINI_API_KEY` / `GOOGLE_API_KEY` environment variable set

## Data sources, by confidence

| Confidence | Source | Notes |
|---|---|---|
| ★★★★★ Official API | Google AI Studio usage endpoint | Available when an API key is configured. |
| ★★★☆☆ CLI log | Gemini CLI local state | Rate-limit headers/state surfaced by the CLI itself. |
| ★★☆☆☆ Browser | gemini.google.com usage panel | Consumer plan (Gemini Advanced) daily/weekly caps. |

## Limit windows reported (once implemented)

- `daily` — free-tier / consumer daily message cap
- `weekly` — Advanced plan weekly cap
- `tokens` — API token spend

## Status

`detect()` implemented, `fetch_status()` is a TODO.
