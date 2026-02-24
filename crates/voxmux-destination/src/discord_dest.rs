use crate::dest_trait::Destination;
use async_trait::async_trait;
use voxmux_core::{DestinationError, TextMetadata};

pub struct DiscordDestination {
    token: Option<String>,
    guild_id: Option<u64>,
    channel_id: Option<u64>,
}

impl DiscordDestination {
    pub fn new() -> Self {
        Self {
            token: None,
            guild_id: None,
            channel_id: None,
        }
    }
}

impl Default for DiscordDestination {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Destination for DiscordDestination {
    fn name(&self) -> &str {
        "discord"
    }

    async fn initialize(&mut self, config: toml::Value) -> Result<(), DestinationError> {
        let token = config
            .get("token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                DestinationError::InitializationFailed("missing 'token' in config".to_string())
            })?;
        let guild_id = config
            .get("guild_id")
            .and_then(|v| v.as_integer())
            .map(|v| v as u64);
        let channel_id = config
            .get("channel_id")
            .and_then(|v| v.as_integer())
            .map(|v| v as u64);

        self.token = Some(token.to_string());
        self.guild_id = guild_id;
        self.channel_id = channel_id;

        tracing::info!("DiscordDestination initialized (stub)");
        Ok(())
    }

    async fn send_text(
        &self,
        text: &str,
        metadata: &TextMetadata,
    ) -> Result<(), DestinationError> {
        tracing::debug!(
            input_id = %metadata.input_id,
            "DiscordDestination stub send: {}{}",
            metadata.prefix,
            text,
        );
        Ok(())
    }

    fn is_healthy(&self) -> bool {
        self.token.is_some()
    }

    async fn shutdown(&self) -> Result<(), DestinationError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discord_dest_name() {
        let dest = DiscordDestination::new();
        assert_eq!(dest.name(), "discord");
    }

    #[tokio::test]
    async fn test_discord_dest_initialize_missing_token_fails() {
        let mut dest = DiscordDestination::new();
        let config = toml::Value::Table(Default::default());
        let result = dest.initialize(config).await;
        match result {
            Err(DestinationError::InitializationFailed(msg)) => {
                assert!(msg.contains("token"));
            }
            _ => panic!("expected InitializationFailed"),
        }
    }

    #[tokio::test]
    async fn test_discord_dest_initialize_with_config_succeeds() {
        let mut dest = DiscordDestination::new();
        let config = toml::Value::Table({
            let mut t = toml::map::Map::new();
            t.insert("token".to_string(), toml::Value::String("bot-token".to_string()));
            t.insert("guild_id".to_string(), toml::Value::Integer(12345));
            t.insert("channel_id".to_string(), toml::Value::Integer(67890));
            t
        });
        let result = dest.initialize(config).await;
        assert!(result.is_ok());
        assert!(dest.is_healthy());
    }

    #[tokio::test]
    async fn test_discord_dest_send_text_stub_succeeds() {
        let mut dest = DiscordDestination::new();
        let config = toml::Value::Table({
            let mut t = toml::map::Map::new();
            t.insert("token".to_string(), toml::Value::String("bot-token".to_string()));
            t
        });
        dest.initialize(config).await.unwrap();

        let metadata = TextMetadata {
            input_id: "mic1".to_string(),
            prefix: "[M1] ".to_string(),
        };
        let result = dest.send_text("hello", &metadata).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_discord_dest_implements_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DiscordDestination>();
    }
}
