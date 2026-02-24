use crate::engine_trait::AsrEngine;
use asr_core::AsrError;
use std::collections::HashMap;

pub struct PluginRegistry {
    factories: HashMap<String, fn() -> Box<dyn AsrEngine>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            factories: HashMap::new(),
        };
        registry.register("null", || Box::new(crate::null_engine::NullEngine::new()));
        #[cfg(feature = "whisper")]
        registry.register("whisper", || {
            Box::new(crate::whisper_engine::WhisperEngine::new())
        });
        registry
    }

    pub fn register(&mut self, name: &str, factory: fn() -> Box<dyn AsrEngine>) {
        self.factories.insert(name.to_string(), factory);
    }

    pub fn create(&self, name: &str) -> Result<Box<dyn AsrEngine>, AsrError> {
        self.factories
            .get(name)
            .map(|f| f())
            .ok_or_else(|| AsrError::EngineNotFound(name.to_string()))
    }

    pub fn list_engines(&self) -> Vec<&str> {
        self.factories.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NullEngine;

    #[test]
    fn test_registry_new_has_null_engine() {
        let registry = PluginRegistry::new();
        assert!(registry.create("null").is_ok());
    }

    #[test]
    fn test_registry_create_null_returns_correct_name() {
        let registry = PluginRegistry::new();
        let engine = registry.create("null").unwrap();
        assert_eq!(engine.name(), "null");
    }

    #[test]
    fn test_registry_create_unknown_returns_error() {
        let registry = PluginRegistry::new();
        let result = registry.create("nope");
        match result {
            Err(AsrError::EngineNotFound(name)) => assert_eq!(name, "nope"),
            _ => panic!("expected EngineNotFound error"),
        }
    }

    #[test]
    fn test_registry_register_custom_engine() {
        let mut registry = PluginRegistry::new();
        registry.register("custom", || Box::new(NullEngine::new()));
        let engine = registry.create("custom").unwrap();
        // NullEngine is used as the factory, so name is still "null"
        assert_eq!(engine.name(), "null");
    }

    #[test]
    fn test_registry_list_engines_includes_null() {
        let registry = PluginRegistry::new();
        let engines = registry.list_engines();
        assert!(engines.contains(&"null"));
    }

    #[test]
    fn test_registry_register_overwrites() {
        let mut registry = PluginRegistry::new();
        // Register a new factory under the same name
        registry.register("null", || Box::new(NullEngine::new()));
        // Should still work (overwritten with same factory type)
        let engine = registry.create("null").unwrap();
        assert_eq!(engine.name(), "null");
    }
}
