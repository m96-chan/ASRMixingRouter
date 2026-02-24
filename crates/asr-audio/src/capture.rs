use asr_core::AudioError;
use cpal::traits::DeviceTrait;
use cpal::{Device, SampleRate, Stream, StreamConfig};
use ringbuf::traits::Producer;
use ringbuf::HeapProd;
use std::sync::{Arc, Mutex};

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
                },
                err_callback,
                None,
            )
            .map_err(|e| AudioError::StreamBuild(e.to_string()))?;

        Ok(Self { _stream: stream })
    }
}
