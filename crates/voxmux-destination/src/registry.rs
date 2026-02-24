use crate::dest_trait::Destination;
use std::collections::HashMap;
use voxmux_core::DestinationError;

pub struct DestinationRegistry {
    factories: HashMap<String, fn() -> Box<dyn Destination>>,
}

impl DestinationRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            factories: HashMap::new(),
        };
        registry.register("file", || Box::new(crate::file_dest::FileDestination::new()));
        #[cfg(feature = "discord")]
        registry.register("discord", || {
            Box::new(crate::discord_dest::DiscordDestination::new())
        });
        registry
    }

    pub fn register(&mut self, name: &str, factory: fn() -> Box<dyn Destination>) {
        self.factories.insert(name.to_string(), factory);
    }

    pub fn create(&self, name: &str) -> Result<Box<dyn Destination>, DestinationError> {
        self.factories
            .get(name)
            .map(|f| f())
            .ok_or_else(|| DestinationError::NotFound(name.to_string()))
    }

    pub fn list_destinations(&self) -> Vec<&str> {
        self.factories.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for DestinationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileDestination;

    #[test]
    fn test_registry_new_has_file_destination() {
        let registry = DestinationRegistry::new();
        assert!(registry.create("file").is_ok());
    }

    #[test]
    fn test_registry_create_file_returns_correct_name() {
        let registry = DestinationRegistry::new();
        let dest = registry.create("file").unwrap();
        assert_eq!(dest.name(), "file");
    }

    #[test]
    fn test_registry_create_unknown_returns_error() {
        let registry = DestinationRegistry::new();
        let result = registry.create("nope");
        match result {
            Err(DestinationError::NotFound(name)) => assert_eq!(name, "nope"),
            _ => panic!("expected NotFound error"),
        }
    }

    #[test]
    fn test_registry_register_custom_destination() {
        let mut registry = DestinationRegistry::new();
        registry.register("custom", || Box::new(FileDestination::new()));
        let dest = registry.create("custom").unwrap();
        // FileDestination is used as the factory, so name is "file"
        assert_eq!(dest.name(), "file");
    }

    #[test]
    fn test_registry_list_destinations_includes_file() {
        let registry = DestinationRegistry::new();
        let dests = registry.list_destinations();
        assert!(dests.contains(&"file"));
    }

    #[test]
    fn test_registry_register_overwrites() {
        let mut registry = DestinationRegistry::new();
        registry.register("file", || Box::new(FileDestination::new()));
        let dest = registry.create("file").unwrap();
        assert_eq!(dest.name(), "file");
    }
}
