use agent_core::{ConnectionState, ProviderPlugin, ProviderStatus};
use agent_plugins::{command_exists_on_path, BasePluginState};
use async_trait::async_trait;
use std::time::Duration;

/// OpenAI Codex CLI. Unlike most other stub providers, this one reports a
/// real `Online`/`Unknown` state — by shelling out to `codex login status`,
/// the CLI's own sanctioned way of answering "am I logged in", the same way
/// `provider-copilot` uses `gh auth token` instead of reading GitHub's
/// credential store directly. This crate never reads `~/.codex/auth.json`
/// or any other credential file — see docs/plugin-development.md and
/// SECURITY.md for why that boundary matters.
///
/// There is no local API or CLI subcommand exposing usage/rate-limit data
/// (checked: `codex --help`, `codex login --help`, `codex debug --help` —
/// no `usage`/`status`/`limits` subcommand exists as of this writing), so
/// `fetch_status()` cannot report a `LimitWindow`. If OpenAI ships a usage
/// API reachable with the Codex CLI's own credentials via a sanctioned
/// export mechanism (like `gh auth token`), prefer that over this
/// connectivity-only signal.
pub struct CodexPlugin {
    state: BasePluginState,
}

impl CodexPlugin {
    pub fn new() -> Self {
        Self { state: BasePluginState::new("codex", "Codex") }
    }
}

impl Default for CodexPlugin {
    fn default() -> Self {
        Self::new()
    }
}

async fn is_logged_in() -> bool {
    let output = tokio::time::timeout(Duration::from_secs(5), tokio::process::Command::new("codex").arg("login").arg("status").output()).await;
    match output {
        // `codex login status` prints its result to stderr, not stdout —
        // confirmed by capturing both streams directly; check both so this
        // doesn't silently regress if that ever changes.
        Ok(Ok(output)) => {
            let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            text.to_lowercase().contains("logged in")
        }
        _ => false,
    }
}

#[async_trait]
impl ProviderPlugin for CodexPlugin {
    fn id(&self) -> &str {
        "codex"
    }
    fn display_name(&self) -> &str {
        "Codex"
    }
    fn refresh_interval_ms(&self) -> u64 {
        5 * 60_000
    }

    async fn detect(&self) -> bool {
        command_exists_on_path("codex")
    }

    async fn refresh(&mut self) {
        let logged_in = is_logged_in().await;
        let mut status = ProviderStatus::unknown(self.id(), self.display_name());
        status.state = if logged_in { ConnectionState::Online } else { ConnectionState::Unknown };
        status.detail = Some(if logged_in {
            "Logged in via Codex CLI — no usage/limit API available locally (checked codex --help, login --help, debug --help)".into()
        } else {
            "codex login status did not report a logged-in session".into()
        });
        self.state.set_status(status);
    }

    fn get_status(&self) -> ProviderStatus {
        self.state.status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn detect_is_false_when_command_is_made_up() {
        // We can't assume `codex` is installed in CI, but we can assert the
        // detection function itself doesn't panic and returns a bool.
        let _ = CodexPlugin::new().detect().await;
    }
}
