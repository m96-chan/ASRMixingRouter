use voxmux_core::{AudioChunk, AudioError};
use cpal::traits::DeviceTrait;
use cpal::{Device, SampleRate, Stream, StreamConfig};
use ringbuf::traits::Producer;
use ringbuf::HeapProd;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use voxmux_core::InputStatus;

// ── CaptureHandle ─────────────────────────────────────────────

#[derive(Clone)]
pub struct CaptureHandle {
    enabled: Arc<AtomicBool>,
    status: Arc<AtomicU8>,
    id: String,
}

impl CaptureHandle {
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, v: bool) {
        self.enabled.store(v, Ordering::Relaxed);
    }

    pub fn status(&self) -> InputStatus {
        match self.status.load(Ordering::Relaxed) {
            1 => InputStatus::Error,
            2 => InputStatus::Disabled,
            _ => InputStatus::Ok,
        }
    }

    pub fn set_status(&self, s: InputStatus) {
        let v = match s {
            InputStatus::Ok => 0,
            InputStatus::Error => 1,
            InputStatus::Disabled => 2,
        };
        self.status.store(v, Ordering::Relaxed);
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

// ── CaptureNode ───────────────────────────────────────────────

pub struct CaptureNode {
    _stream: Stream,
}

impl CaptureNode {
    pub fn new(
        device: &Device,
        producer: HeapProd<f32>,
        sample_rate: u32,
        channels: u16,
        buffer_size: u32,
        asr_tap: Option<mpsc::UnboundedSender<AudioChunk>>,
        id: &str,
    ) -> Result<(Self, CaptureHandle), AudioError> {
        let config = StreamConfig {
            channels,
            sample_rate: SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Fixed(buffer_size),
        };

        let producer = Arc::new(Mutex::new(producer));
        let enabled = Arc::new(AtomicBool::new(true));
        let enabled_flag = Arc::clone(&enabled);
        let status = Arc::new(AtomicU8::new(0));
        let status_flag = Arc::clone(&status);

        let err_callback = move |err: cpal::StreamError| {
            tracing::error!("capture stream error: {}", err);
            status_flag.store(1, Ordering::Relaxed); // Error
        };

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if !enabled_flag.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok(mut prod) = producer.lock() {
                        // Push as much as we can; overflow is silently dropped
                        prod.push_slice(data);
                    }
                    if let Some(ref tap) = asr_tap {
                        let chunk = AudioChunk {
                            samples: data.to_vec(),
                            sample_rate,
                            channels,
                        };
                        let _ = tap.send(chunk);
                    }
                },
                err_callback,
                None,
            )
            .map_err(|e| AudioError::StreamBuild(e.to_string()))?;

        let handle = CaptureHandle {
            enabled,
            status,
            id: id.to_string(),
        };
        Ok((Self { _stream: stream }, handle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxmux_core::AudioChunk;
    use tokio::sync::mpsc;

    fn make_capture_handle(id: &str) -> CaptureHandle {
        CaptureHandle {
            enabled: Arc::new(AtomicBool::new(true)),
            status: Arc::new(AtomicU8::new(0)),
            id: id.to_string(),
        }
    }

    #[test]
    fn test_capture_handle_default_enabled() {
        let handle = make_capture_handle("mic1");
        assert!(handle.is_enabled());
    }

    #[test]
    fn test_capture_handle_disable() {
        let handle = make_capture_handle("mic1");
        handle.set_enabled(false);
        assert!(!handle.is_enabled());
        handle.set_enabled(true);
        assert!(handle.is_enabled());
    }

    #[test]
    fn test_capture_handle_clone_shares_state() {
        let h1 = make_capture_handle("mic1");
        let h2 = h1.clone();
        h1.set_enabled(false);
        assert!(!h2.is_enabled());
    }

    #[test]
    fn test_capture_handle_status_default_ok() {
        let handle = make_capture_handle("mic1");
        assert_eq!(handle.status(), InputStatus::Ok);
    }

    #[test]
    fn test_capture_handle_set_error_status() {
        let handle = make_capture_handle("mic1");
        handle.set_status(InputStatus::Error);
        assert_eq!(handle.status(), InputStatus::Error);
        handle.set_status(InputStatus::Ok);
        assert_eq!(handle.status(), InputStatus::Ok);
    }

    #[test]
    fn test_asr_tap_send_receives_chunk() {
        let (tx, mut rx) = mpsc::unbounded_channel::<AudioChunk>();
        let chunk = AudioChunk {
            samples: vec![0.1, 0.2, 0.3],
            sample_rate: 48000,
            channels: 1,
        };
        tx.send(chunk).unwrap();

        let received = rx.try_recv().unwrap();
        assert_eq!(received.samples, vec![0.1, 0.2, 0.3]);
        assert_eq!(received.sample_rate, 48000);
        assert_eq!(received.channels, 1);
    }

    #[test]
    fn test_asr_tap_none_does_not_panic() {
        let tap: Option<mpsc::UnboundedSender<AudioChunk>> = None;
        // Simulating the callback logic
        if let Some(ref tx) = tap {
            let chunk = AudioChunk {
                samples: vec![0.0],
                sample_rate: 48000,
                channels: 1,
            };
            let _ = tx.send(chunk);
        }
        // No panic — test passes
    }

    #[test]
    fn test_asr_tap_dropped_receiver_does_not_panic() {
        let (tx, rx) = mpsc::unbounded_channel::<AudioChunk>();
        drop(rx);
        let chunk = AudioChunk {
            samples: vec![0.0; 480],
            sample_rate: 48000,
            channels: 1,
        };
        // `let _ = tx.send(...)` should not panic even with a dropped receiver
        let _ = tx.send(chunk);
    }
}
