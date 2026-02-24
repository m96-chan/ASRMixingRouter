use crate::engine_trait::AsrEngine;
use asr_core::{AsrError, AudioChunk, RecognitionResult};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use tokio::sync::mpsc;

pub struct NullEngine {
    feed_count: AtomicUsize,
    result_sender: Mutex<Option<mpsc::UnboundedSender<RecognitionResult>>>,
}

impl NullEngine {
    pub fn new() -> Self {
        Self {
            feed_count: AtomicUsize::new(0),
            result_sender: Mutex::new(None),
        }
    }

    pub fn feed_count(&self) -> usize {
        self.feed_count.load(Ordering::Relaxed)
    }
}

impl Default for NullEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AsrEngine for NullEngine {
    fn name(&self) -> &str {
        "null"
    }

    async fn initialize(&mut self, _config: toml::Value) -> Result<(), AsrError> {
        Ok(())
    }

    async fn feed_audio(&self, chunk: AudioChunk) -> Result<(), AsrError> {
        let count = self.feed_count.fetch_add(1, Ordering::Relaxed) + 1;
        let result = RecognitionResult {
            text: format!("[null] {} samples", chunk.samples.len()),
            input_id: String::new(),
            timestamp: 0.0,
            is_final: true,
        };
        if let Ok(sender) = self.result_sender.lock() {
            if let Some(tx) = sender.as_ref() {
                let _ = tx.send(result);
            }
        }
        tracing::trace!("NullEngine fed chunk #{count}, {} samples", chunk.samples.len());
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
    fn test_null_engine_name() {
        let engine = NullEngine::new();
        assert_eq!(engine.name(), "null");
    }

    #[tokio::test]
    async fn test_null_engine_initialize_succeeds() {
        let mut engine = NullEngine::new();
        let result = engine.initialize(toml::Value::Table(Default::default())).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_null_engine_feed_audio_no_sender() {
        let engine = NullEngine::new();
        let chunk = AudioChunk {
            samples: vec![0.0; 480],
            sample_rate: 48000,
            channels: 1,
        };
        // Should not panic without a sender
        let result = engine.feed_audio(chunk).await;
        assert!(result.is_ok());
        assert_eq!(engine.feed_count(), 1);
    }

    #[tokio::test]
    async fn test_null_engine_feed_audio_sends_result() {
        let mut engine = NullEngine::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        engine.set_result_sender(tx);

        let chunk = AudioChunk {
            samples: vec![0.0; 480],
            sample_rate: 48000,
            channels: 1,
        };
        engine.feed_audio(chunk).await.unwrap();

        let result = rx.recv().await.unwrap();
        assert_eq!(result.text, "[null] 480 samples");
    }

    #[tokio::test]
    async fn test_null_engine_result_has_correct_fields() {
        let mut engine = NullEngine::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        engine.set_result_sender(tx);

        let chunk = AudioChunk {
            samples: vec![0.0; 100],
            sample_rate: 16000,
            channels: 1,
        };
        engine.feed_audio(chunk).await.unwrap();

        let result = rx.recv().await.unwrap();
        assert!(result.is_final);
        assert!(!result.text.is_empty());
    }

    #[tokio::test]
    async fn test_null_engine_feed_count_increments() {
        let engine = NullEngine::new();
        for _ in 0..3 {
            let chunk = AudioChunk {
                samples: vec![0.0; 480],
                sample_rate: 48000,
                channels: 1,
            };
            engine.feed_audio(chunk).await.unwrap();
        }
        assert_eq!(engine.feed_count(), 3);
    }

    #[tokio::test]
    async fn test_null_engine_shutdown_succeeds() {
        let engine = NullEngine::new();
        let result = engine.shutdown().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_null_engine_implements_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NullEngine>();
    }
}
