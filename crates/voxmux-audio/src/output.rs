use voxmux_core::AudioError;
use cpal::traits::DeviceTrait;
use cpal::{Device, SampleRate, Stream, StreamConfig};
use ringbuf::traits::Consumer;
use ringbuf::HeapCons;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use voxmux_core::InputStatus;

const STATUS_OK: u8 = 0;
const STATUS_ERROR: u8 = 1;

// ── OutputHandle ──────────────────────────────────────────────

#[derive(Clone)]
pub struct OutputHandle {
    playing: Arc<AtomicBool>,
    status: Arc<AtomicU8>,
}

impl OutputHandle {
    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn set_playing(&self, v: bool) {
        self.playing.store(v, Ordering::Relaxed);
    }

    pub fn status(&self) -> InputStatus {
        match self.status.load(Ordering::Relaxed) {
            STATUS_ERROR => InputStatus::Error,
            _ => InputStatus::Ok,
        }
    }
}

// ── OutputNode ────────────────────────────────────────────────

pub struct OutputNode {
    _stream: Stream,
}

impl OutputNode {
    pub fn new(
        device: &Device,
        consumer: HeapCons<f32>,
        sample_rate: u32,
        channels: u16,
        buffer_size: u32,
    ) -> Result<(Self, OutputHandle), AudioError> {
        let config = StreamConfig {
            channels,
            sample_rate: SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Fixed(buffer_size),
        };

        let consumer = Arc::new(Mutex::new(consumer));
        let playing = Arc::new(AtomicBool::new(true));
        let playing_flag = Arc::clone(&playing);
        let status = Arc::new(AtomicU8::new(STATUS_OK));
        let status_flag = Arc::clone(&status);

        let err_callback = move |err: cpal::StreamError| {
            tracing::error!("output stream error: {}", err);
            status_flag.store(STATUS_ERROR, Ordering::Relaxed);
        };

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    if !playing_flag.load(Ordering::Relaxed) {
                        data.fill(0.0);
                        return;
                    }
                    if let Ok(mut cons) = consumer.lock() {
                        for sample in data.iter_mut() {
                            *sample = cons.try_pop().unwrap_or(0.0);
                        }
                    } else {
                        // Mutex poisoned — fill with silence
                        data.fill(0.0);
                    }
                },
                err_callback,
                None,
            )
            .map_err(|e| AudioError::StreamBuild(e.to_string()))?;

        let handle = OutputHandle { playing, status };
        Ok((Self { _stream: stream }, handle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_output_handle() -> OutputHandle {
        OutputHandle {
            playing: Arc::new(AtomicBool::new(true)),
            status: Arc::new(AtomicU8::new(STATUS_OK)),
        }
    }

    #[test]
    fn test_output_handle_default_playing() {
        let handle = make_output_handle();
        assert!(handle.is_playing());
    }

    #[test]
    fn test_output_handle_set_playing() {
        let handle = make_output_handle();
        handle.set_playing(false);
        assert!(!handle.is_playing());
        handle.set_playing(true);
        assert!(handle.is_playing());
    }

    #[test]
    fn test_output_handle_clone_shares_state() {
        let h1 = make_output_handle();
        let h2 = h1.clone();
        h1.set_playing(false);
        assert!(!h2.is_playing());
    }

    #[test]
    fn test_output_handle_status_default_ok() {
        let handle = make_output_handle();
        assert_eq!(handle.status(), InputStatus::Ok);
    }
}
