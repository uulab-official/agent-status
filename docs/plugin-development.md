# Writing a provider plugin

This is the practical guide. For the "why" behind the shapes involved, read
[architecture.md](architecture.md) and [data-model.md](data-model.md) first.

**Start by reading [`crates/providers/ollama/src/lib.rs`](../crates/providers/ollama/src/lib.rs).**
It's the one provider with a fully working `fetch_status()` — every stub
provider under `crates/providers/` has `detect()` implemented but leaves
`fetch_status()`'s body as a documented TODO, because it needs either an API
key the maintainers don't have or a scraping strategy that needs to be built
and kept honest about its `Confidence` tier.

## 1. Scaffold the crate

```
crates/providers/<name>/
├── Cargo.toml       # depends on agent-core + agent-plugins (+ reqwest/serde if it calls an API)
└── src/
    └── lib.rs
```

Copy `crates/providers/openrouter` as a starting template if your provider
is a single authenticated API call — it's the simplest complete example.
Add the new crate to the root `Cargo.toml`'s `[workspace.members]` and
`[workspace.dependencies]`.

## 2. Compose `BasePluginState` and implement `ProviderPlugin`

Rust has no class inheritance, so there's no literal `BasePlugin` to extend
the way the original TypeScript prototype had — instead, your plugin struct
*holds* a `BasePluginState` and implements the `ProviderPlugin` trait
directly:

```rust
use agent_core::{ConnectionState, ProviderPlugin, ProviderStatus};
use agent_plugins::BasePluginState;
use async_trait::async_trait;

pub struct MyProviderPlugin {
    state: BasePluginState,
}

impl MyProviderPlugin {
    pub fn new() -> Self {
        Self { state: BasePluginState::new("myprovider", "My Provider") }
    }
}

#[async_trait]
impl ProviderPlugin for MyProviderPlugin {
    fn id(&self) -> &str { "myprovider" }
    fn display_name(&self) -> &str { "My Provider" }
    fn refresh_interval_ms(&self) -> u64 { 60_000 } // pick based on cost/rate-limit sensitivity

    async fn detect(&self) -> bool {
        // Cheap, side-effect-free check. Never panic; return false on any doubt.
        todo!()
    }

    async fn refresh(&mut self) {
        match self.fetch_status().await {
            Ok(status) => self.state.set_status(status),
            Err(e) => self.state.set_error(e), // degrades to ConnectionState::Unknown automatically
        }
    }

    fn get_status(&self) -> ProviderStatus {
        self.state.status()
    }
}

impl MyProviderPlugin {
    async fn fetch_status(&self) -> Result<ProviderStatus, String> {
        // Do the real IO here. Returning Err is fine — refresh() above
        // handles degrading to Unknown with the error in `detail`.
        todo!()
    }
}
```

`get_limits()`, `get_usage()`, `get_models()`, and `drain_notifications()`
all have default trait-method implementations that read from `get_status()`
— you only need to override them if a provider needs something unusual.

## 3. `detect()` checklist

Use the helpers in `agent-plugins`:
- `command_exists_on_path("some-cli")` — is a CLI on `$PATH`?
- `file_exists(&path)` — does a config/state file exist?
- `read_json_file_if_exists::<T>(&path)` — read+parse, returns `None` on any failure

Or check an environment variable (`std::env::var("SOME_API_KEY")`) for
API-key-based providers. Whatever you check, it must:
- Never panic
- Return `false` on any doubt (a false negative just means the provider
  doesn't show up; a false positive means the app tries to poll a provider
  that isn't there, generating noisy `Unknown` states)

## 4. `fetch_status()` checklist

- Pick the **highest-confidence source you can actually implement** — see
  [confidence.md](confidence.md) for the tier definitions and the rules
  around not overstating confidence.
- Map whatever the provider returns into `ProviderStatus`. Look at
  `crates/providers/ollama/src/lib.rs` for a full real example (including a
  concurrent double-fetch with `tokio::join!`) and
  `crates/providers/custom/src/lib.rs` for one with a bearer-token header.
- Every `LimitWindow` and `CostSnapshot` needs a `confidence` — this isn't
  optional; the struct fields aren't `Option` for it.
- Let errors propagate as `Err`. Don't catch-and-return-empty inside
  `fetch_status()` — `refresh()`'s `match` already handles that by calling
  `BasePluginState::set_error`, which sets `ConnectionState::Unknown` and
  records the error in `detail`.
- **Verify the API actually behaves as documented before assuming your code
  is broken.** The Copilot plugin's TODO is there specifically because
  testing against a real `gh auth token` showed `GET /user/copilot/usage`
  404s for individual accounts — that's a real finding to leave as a comment
  for the next person, not a bug to silently "fix" by guessing at a
  different endpoint shape.
- **If there's no usage API but the provider has a CLI, check for a
  sanctioned "am I logged in" command before giving up on reporting
  anything.** `crates/providers/codex` and `crates/providers/cursor` shell
  out to `codex login status` / `cursor-agent status` respectively — real
  connectivity (`ConnectionState::Online` at ★★★☆☆ `CliLog`) is more honest
  than a blanket `Unknown`, even with no `LimitWindow` to show. **Never**
  get there by opening the CLI's own credential file (e.g.
  `~/.codex/auth.json`) to check for a token's presence — see SECURITY.md
  for why that's a hard line, not a style preference. When you test one of
  these, remember to check **both stdout and stderr**: `codex login status`
  prints its result to stderr, not stdout, which is exactly the kind of
  thing that silently breaks a `.stdout`-only check without erroring.

## 5. Write the README

Every provider README follows the same shape (copy `crates/providers/gemini/README.md`
if migrating from the old layout, or `crates/providers/openrouter` for a
fully-implemented example):
1. One-line description of what it monitors
2. Detection strategy
3. A confidence table: tier → source → notes
4. What limit windows / fields it reports
5. Status line (scaffolded vs. implemented) so contributors don't need to
   read the source to know what's left

## 6. Register it in the app

Add your plugin to `create_default_registry()` in
[`src-tauri/src/builtins.rs`](../src-tauri/src/builtins.rs) — this is the
one place allowed to import every provider crate (see
[architecture.md](architecture.md#why-cratesplugins-common-doesnt-depend-on-the-provider-crates)
for why that composition root doesn't live in `agent-plugins`).

If your provider needs **required per-instance config** (a URL, an account
id) rather than just environment auto-detection, follow the
`provider-custom` pattern instead: define a config struct, accept it in
`new(config)`, and have the app construct it directly (e.g. from a future
Settings screen) rather than through `create_default_registry()`'s
auto-detection sweep.

## 7. Tests

Not every provider needs exhaustive tests yet — `detect()` logic that's pure
environment/filesystem checks is worth covering (see
`crates/providers/openai/src/lib.rs`'s tests for the pattern: set/unset an
env var, assert both branches — note the `static ENV_LOCK: Mutex<()>` used
to serialize tests that mutate process-wide env vars, since `cargo test`
runs tests in parallel by default). `fetch_status()` implementations that
call `reqwest` should mock the HTTP layer with `wiremock` — see
`crates/providers/ollama/src/lib.rs` and `crates/providers/openrouter/src/lib.rs`
for the pattern (spin up a `MockServer`, inject its URL via a
`with_base_url`/`with_api_base_and_key` constructor rather than a global env
var, so parallel tests never race on shared mutable state).

## A real bug we hit

While porting the popover's settings UI, a click on a mode button sometimes
didn't visibly update the popover, even though the Rust command handler ran
successfully and persisted to SQLite. The cause: `ui/popover.js`'s
`listen("status-update", ...)` call is itself async (registering an event
listener is a round-trip to the Rust side), so an event emitted right after
a command completes can race ahead of that registration and get silently
missed. The fix (see `ui/popover.js`) is for every settings command to
explicitly re-`invoke("get_view_model")` after it resolves, rather than
relying solely on the pushed event. Keep this in mind if you add new
UI-triggered commands that need an immediate visible result.
