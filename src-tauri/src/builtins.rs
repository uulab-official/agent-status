use agent_core::{PluginRegistry, ProviderPlugin};

/// Detects and registers every built-in provider that's actually usable on
/// this machine. A provider failing `detect()` is expected and silent — most
/// users will have most providers unavailable.
///
/// `provider-custom` is intentionally excluded: it takes required
/// per-instance config (a base URL), so it would be registered directly by a
/// future Settings screen once one exists, not auto-detected here.
pub async fn create_default_registry() -> PluginRegistry {
    let candidates: Vec<Box<dyn ProviderPlugin>> = vec![
        Box::new(provider_claude::ClaudePlugin::new()),
        Box::new(provider_openai::OpenAiPlugin::new()),
        Box::new(provider_gemini::GeminiPlugin::new()),
        Box::new(provider_cursor::CursorPlugin::new()),
        Box::new(provider_copilot::CopilotPlugin::new()),
        Box::new(provider_codex::CodexPlugin::new()),
        Box::new(provider_antigravity::AntigravityPlugin::new()),
        Box::new(provider_ollama::OllamaPlugin::new()),
        Box::new(provider_openrouter::OpenRouterPlugin::new()),
    ];

    let mut registry = PluginRegistry::new();
    for plugin in candidates {
        if plugin.detect().await {
            registry.register(plugin);
        }
    }
    registry
}
