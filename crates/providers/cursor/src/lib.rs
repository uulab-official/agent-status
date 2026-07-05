use agent_core::{ConnectionState, ProviderPlugin, ProviderStatus};
use agent_plugins::{command_exists_on_path, file_exists, BasePluginState};
use async_trait::async_trait;
use std::time::Duration;

fn cursor_config_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|home| home.join(".cursor"))
}

/// Runs `cursor-agent status`, the CLI's own sanctioned way of answering "am
/// I logged in" — this crate never reads `~/.cursor`'s stored session data
/// directly. Same pattern as `provider-copilot`'s `gh auth token` and
/// `provider-codex`'s `codex login status`. See SECURITY.md.
async fn is_logged_in() -> bool {
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new("cursor-agent").arg("status").output(),
    )
    .await;
    match output {
        Ok(Ok(output)) => {
            let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            text.to_lowercase().contains("logged in")
        }
        _ => false,
    }
}

/// Cursor editor (Premium/Slow request usage). See README.md for the
/// confidence tiers a real quota reading would need — no such reading is
/// available today; see `refresh()`.
pub struct CursorPlugin {
    state: BasePluginState,
}

impl CursorPlugin {
    pub fn new() -> Self {
        Self { state: BasePluginState::new("cursor", "Cursor") }
    }
}

impl Default for CursorPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProviderPlugin for CursorPlugin {
    fn id(&self) -> &str {
        "cursor"
    }
    fn display_name(&self) -> &str {
        "Cursor"
    }
    fn refresh_interval_ms(&self) -> u64 {
        5 * 60_000
    }

    async fn detect(&self) -> bool {
        let has_config_dir = cursor_config_dir().map(|dir| file_exists(&dir)).unwrap_or(false);
        has_config_dir || command_exists_on_path("cursor-agent")
    }

    async fn refresh(&mut self) {
        // Real Premium/Slow quota numbers would need the cursor.com
        // dashboard's usage response, which requires a session cookie.
        // Investigated `cursor-agent`'s CLI surface first for a sanctioned
        // way to get one — it has no `usage`/`limits` subcommand, only
        // `status`/`whoami`, which reports login state but not quota. The
        // remaining option (reading the session cookie out of `~/.cursor`
        // directly) is the same "open another tool's credential store"
        // pattern this project rejected for Claude's Keychain-stored OAuth
        // token — see SECURITY.md and ROADMAP.md's Claude entry. Not
        // pursuing it here either; this stays connectivity-only (★★★☆☆)
        // until Cursor ships a sanctioned way to read quota.
        let logged_in = is_logged_in().await;
        let mut status = ProviderStatus::unknown(self.id(), self.display_name());
        status.state = if logged_in { ConnectionState::Online } else { ConnectionState::Unknown };
        status.detail = Some(if logged_in {
            "Logged in via cursor-agent CLI — no usage/limit API available locally (cursor-agent has no usage subcommand)".into()
        } else {
            "cursor-agent status did not report a logged-in session".into()
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
    async fn detect_does_not_panic() {
        let _ = CursorPlugin::new().detect().await;
    }
}
