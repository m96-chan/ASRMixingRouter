use voxmux_core::{AudioChunk, AudioError};
use cpal::traits::DeviceTrait;
use cpal::{Device, SampleRate, Stream, StreamConfig};
use ringbuf::traits::Producer;
use ringbuf::HeapProd;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

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
    ) -> Result<Self, AudioError> {
        let config = StreamConfig {
            channels,
            sample_rate: SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Fixed(buffer_size),
        };

        let producer = Arc::new(Mutex::new(producer));

        let err_callback = |err: cpal::StreamError| {
            tracing::error!("capture stream error: {}", err);
        };

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
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

        Ok(Self { _stream: stream })
    }
}

#[cfg(test)]
mod tests {
    use voxmux_core::AudioChunk;
    use tokio::sync::mpsc;

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
