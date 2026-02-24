use crate::engine_trait::AsrEngine;
use crate::registry::PluginRegistry;
use asr_core::{AsrError, AudioChunk, RecognitionResult};
use tokio::sync::mpsc;

struct PendingInput {
    id: String,
    engine: Box<dyn AsrEngine>,
    tap_rx: mpsc::UnboundedReceiver<AudioChunk>,
    engine_result_rx: mpsc::UnboundedReceiver<RecognitionResult>,
}

pub struct AsrHost {
    inputs: Vec<PendingInput>,
    result_tx: mpsc::UnboundedSender<RecognitionResult>,
    result_rx: Option<mpsc::UnboundedReceiver<RecognitionResult>>,
    task_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl AsrHost {
    pub fn new() -> Self {
        let (result_tx, result_rx) = mpsc::unbounded_channel();
        Self {
            inputs: Vec::new(),
            result_tx,
            result_rx: Some(result_rx),
            task_handles: Vec::new(),
        }
    }

    pub fn take_result_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<RecognitionResult>> {
        self.result_rx.take()
    }

    pub async fn add_input(
        &mut self,
        id: &str,
        engine_name: &str,
        config: toml::Value,
        registry: &PluginRegistry,
    ) -> Result<mpsc::UnboundedSender<AudioChunk>, AsrError> {
        let mut engine = registry.create(engine_name)?;

        // Create per-engine result channel
        let (engine_result_tx, engine_result_rx) = mpsc::unbounded_channel();
        engine.set_result_sender(engine_result_tx);
        engine.initialize(config).await?;

        // Create tap channel for audio input
        let (tap_tx, tap_rx) = mpsc::unbounded_channel();

        self.inputs.push(PendingInput {
            id: id.to_string(),
            engine,
            tap_rx,
            engine_result_rx,
        });

        Ok(tap_tx)
    }

    pub fn start(&mut self) {
        let inputs = std::mem::take(&mut self.inputs);
        for input in inputs {
            let input_id = input.id;
            let engine = input.engine;
            let mut tap_rx = input.tap_rx;
            let mut engine_result_rx = input.engine_result_rx;
            let shared_tx = self.result_tx.clone();

            let handle = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        chunk = tap_rx.recv() => {
                            match chunk {
                                Some(audio) => {
                                    if let Err(e) = engine.feed_audio(audio).await {
                                        tracing::error!(
                                            input_id = %input_id,
                                            "engine feed error: {e}"
                                        );
                                    }
                                }
                                None => {
                                    // Tap sender dropped — shut down this input
                                    tracing::debug!(
                                        input_id = %input_id,
                                        "tap sender dropped, shutting down"
                                    );
                                    let _ = engine.shutdown().await;
                                    break;
                                }
                            }
                        }
                        result = engine_result_rx.recv() => {
                            match result {
                                Some(mut r) => {
                                    r.input_id = input_id.clone();
                                    let _ = shared_tx.send(r);
                                }
                                None => {
                                    // Engine result channel closed
                                    break;
                                }
                            }
                        }
                    }
                }
            });
            self.task_handles.push(handle);
        }
    }

    pub async fn shutdown(&mut self) {
        let handles = std::mem::take(&mut self.task_handles);
        for handle in handles {
            let _ = handle.await;
        }
    }
}

impl Default for AsrHost {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> PluginRegistry {
        PluginRegistry::new()
    }

    #[tokio::test]
    async fn test_host_new_has_result_receiver() {
        let mut host = AsrHost::new();
        assert!(host.take_result_receiver().is_some());
        assert!(host.take_result_receiver().is_none());
    }

    #[tokio::test]
    async fn test_host_add_input_returns_tap_sender() {
        let mut host = AsrHost::new();
        let registry = test_registry();
        let tx = host
            .add_input("mic1", "null", toml::Value::Table(Default::default()), &registry)
            .await
            .unwrap();
        // Sending should not panic
        let chunk = AudioChunk {
            samples: vec![0.0; 480],
            sample_rate: 48000,
            channels: 1,
        };
        tx.send(chunk).unwrap();
    }

    #[tokio::test]
    async fn test_host_add_input_unknown_engine_fails() {
        let mut host = AsrHost::new();
        let registry = test_registry();
        let result = host
            .add_input("mic1", "nonexistent", toml::Value::Table(Default::default()), &registry)
            .await;
        match result {
            Err(AsrError::EngineNotFound(_)) => {}
            _ => panic!("expected EngineNotFound"),
        }
    }

    #[tokio::test]
    async fn test_host_start_and_feed_produces_result() {
        let mut host = AsrHost::new();
        let registry = test_registry();
        let mut rx = host.take_result_receiver().unwrap();

        let tx = host
            .add_input("mic1", "null", toml::Value::Table(Default::default()), &registry)
            .await
            .unwrap();
        host.start();

        let chunk = AudioChunk {
            samples: vec![0.0; 480],
            sample_rate: 48000,
            channels: 1,
        };
        tx.send(chunk).unwrap();

        let result = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert!(result.text.contains("480"));
    }

    #[tokio::test]
    async fn test_host_multiple_inputs_produce_results() {
        let mut host = AsrHost::new();
        let registry = test_registry();
        let mut rx = host.take_result_receiver().unwrap();

        let tx1 = host
            .add_input("mic1", "null", toml::Value::Table(Default::default()), &registry)
            .await
            .unwrap();
        let tx2 = host
            .add_input("mic2", "null", toml::Value::Table(Default::default()), &registry)
            .await
            .unwrap();
        host.start();

        let chunk1 = AudioChunk {
            samples: vec![0.0; 100],
            sample_rate: 48000,
            channels: 1,
        };
        let chunk2 = AudioChunk {
            samples: vec![0.0; 200],
            sample_rate: 48000,
            channels: 1,
        };
        tx1.send(chunk1).unwrap();
        tx2.send(chunk2).unwrap();

        let timeout = std::time::Duration::from_secs(2);
        let r1 = tokio::time::timeout(timeout, rx.recv())
            .await
            .expect("timed out")
            .expect("closed");
        let r2 = tokio::time::timeout(timeout, rx.recv())
            .await
            .expect("timed out")
            .expect("closed");

        let mut ids: Vec<_> = vec![r1.input_id.clone(), r2.input_id.clone()];
        ids.sort();
        assert_eq!(ids, vec!["mic1", "mic2"]);
    }

    #[tokio::test]
    async fn test_host_drop_tap_sender_stops_task() {
        let mut host = AsrHost::new();
        let registry = test_registry();

        let tx = host
            .add_input("mic1", "null", toml::Value::Table(Default::default()), &registry)
            .await
            .unwrap();
        host.start();

        drop(tx);

        // Shutdown should complete without hanging
        tokio::time::timeout(std::time::Duration::from_secs(2), host.shutdown())
            .await
            .expect("shutdown timed out");
    }

    #[tokio::test]
    async fn test_host_shutdown_awaits_tasks() {
        let mut host = AsrHost::new();
        let registry = test_registry();

        let tx = host
            .add_input("mic1", "null", toml::Value::Table(Default::default()), &registry)
            .await
            .unwrap();
        host.start();

        drop(tx);

        // Should not hang
        tokio::time::timeout(std::time::Duration::from_secs(2), host.shutdown())
            .await
            .expect("shutdown timed out");
    }

    #[tokio::test]
    async fn test_host_result_contains_input_id() {
        let mut host = AsrHost::new();
        let registry = test_registry();
        let mut rx = host.take_result_receiver().unwrap();

        let tx = host
            .add_input("radio1", "null", toml::Value::Table(Default::default()), &registry)
            .await
            .unwrap();
        host.start();

        let chunk = AudioChunk {
            samples: vec![0.0; 480],
            sample_rate: 48000,
            channels: 1,
        };
        tx.send(chunk).unwrap();

        let result = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out")
            .expect("closed");
        assert_eq!(result.input_id, "radio1");
    }
}
