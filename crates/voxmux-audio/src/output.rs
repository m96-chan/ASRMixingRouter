use voxmux_core::AudioError;
use cpal::traits::DeviceTrait;
use cpal::{Device, SampleRate, Stream, StreamConfig};
use ringbuf::traits::Consumer;
use ringbuf::HeapCons;
use std::sync::{Arc, Mutex};

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
    ) -> Result<Self, AudioError> {
        let config = StreamConfig {
            channels,
            sample_rate: SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Fixed(buffer_size),
        };

        let consumer = Arc::new(Mutex::new(consumer));

        let err_callback = |err: cpal::StreamError| {
            tracing::error!("output stream error: {}", err);
        };

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
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

        Ok(Self { _stream: stream })
    }
}
