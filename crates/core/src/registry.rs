use crate::plugin::ProviderPlugin;

/// Holds every enabled `ProviderPlugin` instance and fans out refresh calls.
/// This is the one piece of "core" that touches plugin lifecycles — the tray,
/// popover, and notification engine only ever read from it.
///
/// Backed by a `Vec` rather than a `HashMap`: the provider count is always
/// small (a handful), and a `Vec` keeps registration order for free and
/// avoids the borrow-checker gymnastics of handing out multiple mutable
/// references into a hash map.
#[derive(Default)]
pub struct PluginRegistry {
    plugins: Vec<Box<dyn ProviderPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, plugin: Box<dyn ProviderPlugin>) {
        if let Some(existing) = self.plugins.iter_mut().find(|p| p.id() == plugin.id()) {
            *existing = plugin;
        } else {
            self.plugins.push(plugin);
        }
    }

    pub fn get(&self, id: &str) -> Option<&dyn ProviderPlugin> {
        self.plugins.iter().find(|p| p.id() == id).map(|b| b.as_ref())
    }

    pub fn list(&self) -> Vec<&dyn ProviderPlugin> {
        self.plugins.iter().map(|b| b.as_ref()).collect()
    }

    pub fn list_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn ProviderPlugin>> {
        self.plugins.iter_mut()
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ConnectionState, ProviderStatus};
    use async_trait::async_trait;

    struct FakePlugin {
        id: String,
    }

    #[async_trait]
    impl ProviderPlugin for FakePlugin {
        fn id(&self) -> &str {
            &self.id
        }
        fn display_name(&self) -> &str {
            &self.id
        }
        fn refresh_interval_ms(&self) -> u64 {
            60_000
        }
        async fn detect(&self) -> bool {
            true
        }
        async fn refresh(&mut self) {}
        fn get_status(&self) -> ProviderStatus {
            let mut status = ProviderStatus::unknown(self.id.clone(), self.id.clone());
            status.state = ConnectionState::Online;
            status
        }
    }

    #[test]
    fn registers_and_lists_plugins() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(FakePlugin { id: "claude".into() }));
        assert_eq!(registry.list().len(), 1);
        assert_eq!(registry.get("claude").unwrap().display_name(), "claude");
    }

    #[test]
    fn re_registering_replaces_without_duplicating() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(FakePlugin { id: "claude".into() }));
        registry.register(Box::new(FakePlugin { id: "claude".into() }));
        assert_eq!(registry.list().len(), 1);
    }

    #[test]
    fn preserves_registration_order() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(FakePlugin { id: "b".into() }));
        registry.register(Box::new(FakePlugin { id: "a".into() }));
        let ids: Vec<&str> = registry.list().iter().map(|p| p.id()).collect();
        assert_eq!(ids, vec!["b", "a"]);
    }
}
