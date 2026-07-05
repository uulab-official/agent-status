# provider-openrouter

Reports **OpenRouter** credit balance and spend for the configured API key.

## Detection

`OPENROUTER_API_KEY` environment variable set.

## Data sources, by confidence

| Confidence | Source | Notes |
|---|---|---|
| ★★★★★ Official API | `GET https://openrouter.ai/api/v1/auth/key` | Documented endpoint returning limit, usage, and remaining credits for the calling key. |

## What it reports

- `cost` — `usd` spend (`this_month`) and `credits_remaining`
- `limits` — a `credit` `LimitWindow` (`period: "fixed"`) when the key has an
  explicit spend cap; empty when the key is pay-as-you-go (`limit: null` in
  the API response), since there's no fixed ceiling to bar-chart against
- `state` — `RateLimited` once `usage >= limit`, otherwise `Online`

## Status

Fully implemented and unit-tested against a mocked HTTP server (unlimited
key, capped key, at-limit, and API-error cases). Not yet verified against a
live key in this repo's own testing — if you have an `OPENROUTER_API_KEY`,
running the app is a good way to confirm the real response shape still
matches `KeyResponse` in `src/lib.rs`.
