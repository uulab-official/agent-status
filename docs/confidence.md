# Confidence levels

Every `LimitWindow` and `CostSnapshot` in the standard status model carries a
`Confidence` value (`crates/core/src/types.rs`). This is the project's
answer to a real failure mode: a scraped number that's stale or wrong,
displayed with the same visual weight as an official API response, misleads
a user into over-trusting it right before they get rate-limited.

```rust
pub enum Confidence {
    UserInput = 1,
    BrowserScrape = 2,
    CliLog = 3,
    OfficialScreen = 4,
    OfficialApi = 5,
}
```

| Value | Variant | Source | Example |
|---|---|---|---|
| ★★★★★ (5) | `OfficialApi` | A documented, stable metering endpoint | OpenRouter `GET /api/v1/auth/key` (implemented), Ollama's own local REST API (implemented), OpenAI `/v1/usage`, GitHub Copilot's usage API (both still TODO) |
| ★★★★☆ (4) | `OfficialScreen` | A first-party usage *screen's* backing response, not a documented API contract | Cursor's dashboard usage JSON — real, first-party, but could change shape without notice |
| ★★★☆☆ (3) | `CliLog` | Parsed from a CLI tool's own local logs/state files | Claude Code's `~/.claude` session state, `gh copilot` local state |
| ★★☆☆☆ (2) | `BrowserScrape` | Scraped from a logged-in browser session | claude.ai `/settings/usage`, chat.openai.com's usage panel — the only path when no API/CLI surface exists |
| ★☆☆☆☆ (1) | `UserInput` | Manually entered or user-configured | A user-typed budget, a `base_url` for a self-hosted endpoint |

## Rules for plugin authors

1. **Never invent a higher confidence than your source deserves.** If you
   had to scrape HTML, it's `BrowserScrape`, even if it happens to be
   accurate today.
2. **Prefer the highest available source, but keep the fallback.** A plugin
   can (and often should) try `OfficialApi` first and fall back to
   `BrowserScrape` if the user hasn't configured an API key — just make sure
   the returned `Confidence` reflects whichever path actually ran, not
   whichever path is best-case.
3. **If a provider ships an official API later, upgrade the plugin and
   demote scraping to the fallback**, don't leave both at equal footing.
4. **`UserInput` is not a last resort to avoid writing a real integration.**
   It's for genuinely user-supplied values (a budget, a custom endpoint URL)
   — a provider that only ever returns `UserInput` limits isn't reporting
   anything meaningful.
5. **Don't claim an API works until you've actually verified it responds.**
   The Copilot plugin's TODO comment (`crates/providers/copilot/src/lib.rs`)
   documents a real example: `GET /user/copilot/usage` returns 404 for
   individual accounts even with a valid token, discovered by actually
   testing it against a real `gh auth token` rather than assuming the
   documented endpoint would work.

## How the UI uses this

The popover surfaces confidence indirectly today (via `confidence_stars` in
`LimitRowViewModel`, `src-tauri/src/view_model.rs`) but doesn't render the
stars yet — see [ROADMAP.md](../ROADMAP.md) for the dashboard work that will
make this visible directly (the root README's ★ rating table is the target
UI). When it ships, anything at `BrowserScrape` or below should visually
read as "best effort" rather than "exact."
