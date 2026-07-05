# provider-ollama

Reports local **Ollama** server reachability, installed models, and currently
loaded (running) models with their VRAM footprint.

This is the **reference implementation** — the only provider crate with a
fully working `fetch_status()` — because Ollama's local REST API needs no
auth, no scraping, and no CLI-log parsing. Read this one first when writing
a new plugin. See [docs/plugin-development.md](../../../docs/plugin-development.md).

## Detection

`GET http://localhost:11434/api/tags` responds (default Ollama port; override
via `OLLAMA_HOST`).

## Data sources, by confidence

| Confidence | Source | Notes |
|---|---|---|
| ★★★★★ Official API | Ollama's local REST API (`/api/tags`, `/api/ps`) | Fully documented, no auth, authoritative — it *is* the source of truth. |

## What it reports

Ollama has no usage cap (it's your own hardware), so this plugin reports
`limits: []` and instead surfaces:
- `models` — every locally pulled model, with `is_active` set for whichever
  are currently loaded
- `detail` — a human-readable summary of currently loaded models and their
  VRAM usage, sourced from `/api/ps`

Future work (v2.0, "local model GPU/memory status" on the roadmap): surface
VRAM/RAM as a proper `LimitWindow` once we decide how to represent "percent
of a machine's total memory" without pulling in a native GPU-stats dependency.

## Status

Fully implemented and unit-tested against a mocked HTTP server (`wiremock`),
covering: successful detection, unreachable server, model list with a
running model, and a degraded (500) response from `/api/tags`.
