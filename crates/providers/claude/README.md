# provider-claude

Reports usage for **Claude** — both claude.ai (Desktop/web) and the Claude
Code CLI.

## Detection

- Claude Code CLI on `$PATH` (`claude`)
- `~/.claude` config/state directory present

## Data sources, by confidence

| Confidence | Source | Notes |
|---|---|---|
| ★★★☆☆ CLI log | `~/.claude/projects/**/*.jsonl` session transcripts | Claude Code writes one JSONL file per session with a `message.usage` object per turn (`input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`). This plugin sums those over rolling windows — the same source the open-source `ccusage` CLI and `Claude-Code-Usage-Monitor` use. |
| ★★☆☆☆ Browser | claude.ai `/settings/usage` (logged-in session) | Only source for claude.ai web/desktop usage today; not implemented — CLI transcripts cover the Claude Code case at higher confidence. |

There is currently no official Anthropic usage-metering API for consumer
plans, so this plugin cannot report at ★★★★★, and it cannot show a
**percentage** of the plan's cap — Anthropic doesn't expose the numeric
token ceiling for Pro/Max plans anywhere `fetch_status()` can read it. It
reports raw token counts for each window instead (`limit: None`) rather
than inventing a percentage — see
[docs/confidence.md](../../../docs/confidence.md) for why guessing a cap to
"look done" isn't acceptable here. If Anthropic ships an official metering
API, prefer it and use it to populate `limit`/`percent_used` directly.

Note: `~/.claude/stats-cache.json` has historical daily message/session
counts but no current-window percentage either, so it isn't used here.

## Limit windows reported

- `session` — sum of *effective* tokens across all local session
  transcripts with a timestamp within the last 5 hours (rolling, not a
  fixed block boundary — matches Anthropic's own description of the cap).
- `weekly` — same sum over the last 7 days.

"Effective tokens" weights each field the way Anthropic bills it —
`input_tokens`/`output_tokens` at 1x, `cache_creation_input_tokens` at
1.25x, `cache_read_input_tokens` at 0.1x — rather than summing raw token
counts 1:1. A long agentic session re-sends its cached context on every
turn, so `cache_read_input_tokens` alone can reach the hundreds of millions
within a single 5-hour window; counting it at full weight made the reported
number wildly overstate real usage pressure (a session with a 150k-token
cached context and 200 turns would otherwise report 30M+ tokens for what's
actually a much smaller amount of new work).

Both windows report `used` with `limit: None`, since the real cap isn't
locally observable.

## Status

`detect()` and `fetch_status()` are both implemented
(`crates/providers/claude/src/lib.rs`).
