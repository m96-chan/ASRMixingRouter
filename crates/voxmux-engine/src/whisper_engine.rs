use crate::engine_trait::AsrEngine;
use voxmux_core::{AsrError, AudioChunk, RecognitionResult};
use async_trait::async_trait;
use tokio::sync::mpsc;

pub struct WhisperEngine {
    model_path: Option<String>,
    language: Option<String>,
    result_sender: std::sync::Mutex<Option<mpsc::UnboundedSender<RecognitionResult>>>,
}

impl WhisperEngine {
    pub fn new() -> Self {
        Self {
            model_path: None,
            language: None,
            result_sender: std::sync::Mutex::new(None),
        }
    }
}

impl Default for WhisperEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AsrEngine for WhisperEngine {
    fn name(&self) -> &str {
        "whisper"
    }

    async fn initialize(&mut self, config: toml::Value) -> Result<(), AsrError> {
        let model_path = config
            .get("model_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AsrError::InitializationFailed("missing 'model_path' in whisper config".to_string())
            })?;
        self.model_path = Some(model_path.to_string());

        self.language = config
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        tracing::info!(
            model_path = %model_path,
            language = ?self.language,
            "WhisperEngine initialized (stub — model not loaded)"
        );
        Ok(())
    }

    async fn feed_audio(&self, _chunk: AudioChunk) -> Result<(), AsrError> {
        // Stub: real inference deferred to when whisper-rs is actually wired
        Ok(())
    }

    fn set_result_sender(&mut self, sender: mpsc::UnboundedSender<RecognitionResult>) {
        *self.result_sender.lock().unwrap() = Some(sender);
    }

    async fn shutdown(&self) -> Result<(), AsrError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whisper_engine_name() {
        let engine = WhisperEngine::new();
        assert_eq!(engine.name(), "whisper");
    }

    #[tokio::test]
    async fn test_whisper_engine_initialize_missing_model_path_fails() {
        let mut engine = WhisperEngine::new();
        let result = engine
            .initialize(toml::Value::Table(Default::default()))
            .await;
        match result {
            Err(AsrError::InitializationFailed(msg)) => {
                assert!(msg.contains("model_path"));
            }
            _ => panic!("expected InitializationFailed"),
        }
    }

    #[tokio::test]
    async fn test_whisper_engine_initialize_with_config_succeeds() {
        let mut engine = WhisperEngine::new();
        let mut table = toml::map::Map::new();
        table.insert(
            "model_path".to_string(),
            toml::Value::String("./models/test.bin".to_string()),
        );
        table.insert(
            "language".to_string(),
            toml::Value::String("ja".to_string()),
        );
        let result = engine.initialize(toml::Value::Table(table)).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_whisper_engine_implements_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WhisperEngine>();
    }
}
