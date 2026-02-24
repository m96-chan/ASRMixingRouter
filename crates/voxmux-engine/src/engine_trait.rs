use voxmux_core::{AsrError, AudioChunk, RecognitionResult};
use async_trait::async_trait;
use tokio::sync::mpsc;

/// An ASR (Automatic Speech Recognition) engine that processes audio and emits text.
///
/// Implementations receive audio chunks via [`feed_audio`](Self::feed_audio) and
/// send [`RecognitionResult`]s through the channel provided by
/// [`set_result_sender`](Self::set_result_sender).
#[async_trait]
pub trait AsrEngine: Send + Sync {
    /// Returns the engine's plugin name (e.g. `"null"`, `"whisper"`).
    fn name(&self) -> &str;
    /// One-time initialisation with engine-specific TOML configuration.
    async fn initialize(&mut self, config: toml::Value) -> Result<(), AsrError>;
    /// Feed a chunk of audio samples to the engine for recognition.
    async fn feed_audio(&self, chunk: AudioChunk) -> Result<(), AsrError>;
    /// Set the channel where recognition results will be sent.
    fn set_result_sender(&mut self, sender: mpsc::UnboundedSender<RecognitionResult>);
    /// Gracefully shut down the engine, releasing resources.
    async fn shutdown(&self) -> Result<(), AsrError>;
}
