use voxmux_core::{AsrError, AudioChunk, RecognitionResult};
use async_trait::async_trait;
use tokio::sync::mpsc;

#[async_trait]
pub trait AsrEngine: Send + Sync {
    fn name(&self) -> &str;
    async fn initialize(&mut self, config: toml::Value) -> Result<(), AsrError>;
    async fn feed_audio(&self, chunk: AudioChunk) -> Result<(), AsrError>;
    fn set_result_sender(&mut self, sender: mpsc::UnboundedSender<RecognitionResult>);
    async fn shutdown(&self) -> Result<(), AsrError>;
}
