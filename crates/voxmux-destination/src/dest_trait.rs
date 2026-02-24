use async_trait::async_trait;
use voxmux_core::{DestinationError, TextMetadata};

#[async_trait]
pub trait Destination: Send + Sync {
    fn name(&self) -> &str;
    async fn initialize(&mut self, config: toml::Value) -> Result<(), DestinationError>;
    async fn send_text(&self, text: &str, metadata: &TextMetadata) -> Result<(), DestinationError>;
    fn is_healthy(&self) -> bool;
    async fn shutdown(&self) -> Result<(), DestinationError>;
}
