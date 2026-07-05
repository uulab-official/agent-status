# provider-openai

Reports usage for **OpenAI** — the ChatGPT app/web (message caps, Plus/Pro
limits) and the platform API (token spend).

## Detection

`OPENAI_API_KEY` environment variable set.

## Data sources, by confidence

| Confidence | Source | Notes |
|---|---|---|
| ★★★★★ Official API | `GET /v1/usage` (Organization API) | Token spend and cost, when an API key with usage-read scope is configured. |
| ★★☆☆☆ Browser | chat.openai.com `/settings` usage panel | Only path for ChatGPT Plus/Pro message-cap tracking; no public API for consumer plan limits. |

## Limit windows reported (once implemented)

- `messages` — ChatGPT plan message cap (3-hour rolling, varies by plan)
- `tokens` / `usd` — API spend, daily/monthly

## Status

`detect()` implemented, `fetch_status()` is a TODO.
