# provider-custom

A generic plugin for any server that speaks the OpenAI-compatible
`GET /v1/models` shape — this covers **LM Studio**, **AnythingLLM**,
**Open WebUI**, **Local AI**, and truly custom user-defined endpoints without
writing a new plugin per tool.

## Configuration

```rust
CustomPlugin::new(CustomPluginConfig {
    id: "lmstudio".into(),            // becomes the ProviderId shown in the UI
    display_name: "LM Studio".into(),
    base_url: "http://localhost:1234/v1".into(),
    api_key: None,                     // optional bearer token
});
```

Multiple instances can be registered side by side — one per self-hosted
server the user points the app at. Unlike the auto-detected built-ins, this
one takes required config, so it's constructed directly rather than through
`create_default_registry()`'s detection sweep — see
[docs/plugin-development.md](../../../docs/plugin-development.md#6-register-it-in-the-app).

## Detection

`GET {base_url}/models` responds.

## Data sources, by confidence

| Confidence | Source | Notes |
|---|---|---|
| ★★★★★ Official API | The target server's own `/v1/models` | Self-hosted servers are authoritative about their own model list; there is no usage cap to report for local inference. |
| ★☆☆☆☆ User input | User-supplied `base_url`/`api_key` | The connection itself is only as trustworthy as what the user configured. |

## What it reports

- `models` — whatever the endpoint's `/v1/models` returns
- No `limits` — self-hosted servers are treated as uncapped unless a future
  config option adds a user-defined budget.

## Status

Fully implemented and unit-tested against a mocked HTTP server, including
the bearer-token-header case.
