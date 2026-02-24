use async_trait::async_trait;
use voxmux_core::{DestinationError, TextMetadata};

/// A text destination that receives recognized speech and forwards it somewhere.
///
/// Implementations are registered via [`DestinationRegistry`](crate::DestinationRegistry)
/// and receive text through [`send_text`](Self::send_text) with per-message
/// [`TextMetadata`] (input ID, prefix, etc.).
#[async_trait]
pub trait Destination: Send + Sync {
    /// Returns the destination's plugin name (e.g. `"file"`, `"discord"`).
    fn name(&self) -> &str;
    /// One-time initialisation with destination-specific TOML configuration.
    async fn initialize(&mut self, config: toml::Value) -> Result<(), DestinationError>;
    /// Send recognized text to this destination.
    async fn send_text(&self, text: &str, metadata: &TextMetadata) -> Result<(), DestinationError>;
    /// Returns `true` if the destination is currently able to accept text.
    fn is_healthy(&self) -> bool;
    /// Gracefully shut down the destination, releasing resources.
    async fn shutdown(&self) -> Result<(), DestinationError>;
}
