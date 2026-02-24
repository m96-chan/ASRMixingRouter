use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "asr-mixing-router", about = "Audio mixing router with ASR")]
struct Cli {
    /// Path to the configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let config = asr_core::AppConfig::load_from_file(&cli.config)
        .with_context(|| format!("failed to load config from {:?}", cli.config))?;

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(&config.general.log_level)
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("ASRMixingRouter starting");

    let device_manager = asr_audio::DeviceManager::new();

    // Get input device (first configured input, or default)
    let input_device = if let Some(input_cfg) = config.input.first() {
        tracing::info!("using input device: {}", input_cfg.device_name);
        device_manager
            .get_input_device(&input_cfg.device_name)
            .with_context(|| format!("failed to get input device: {}", input_cfg.device_name))?
    } else {
        tracing::info!("no input configured, using default input device");
        device_manager
            .get_input_device("default")
            .context("failed to get default input device")?
    };

    // Get output device
    tracing::info!("using output device: {}", config.output.device_name);
    let output_device = device_manager
        .get_output_device(&config.output.device_name)
        .with_context(|| {
            format!(
                "failed to get output device: {}",
                config.output.device_name
            )
        })?;

    let sample_rate = config.general.sample_rate;
    let channels: u16 = 1;
    let buffer_size = config.general.buffer_size;

    // Ring buffer capacity: ~2 seconds of audio
    let ring_capacity = (sample_rate as usize) * (channels as usize) * 2;
    let (producer, consumer) = asr_audio::create_ring_buffer(ring_capacity);

    tracing::info!(
        "creating passthrough pipeline: {}Hz, {} ch, buffer={}",
        sample_rate,
        channels,
        buffer_size,
    );

    let _capture = asr_audio::CaptureNode::new(
        &input_device,
        producer,
        sample_rate,
        channels,
        buffer_size,
    )
    .context("failed to create capture node")?;

    let _output = asr_audio::OutputNode::new(
        &output_device,
        consumer,
        sample_rate,
        channels,
        buffer_size,
    )
    .context("failed to create output node")?;

    tracing::info!("passthrough active — press Ctrl+C to stop");

    tokio::signal::ctrl_c()
        .await
        .context("failed to listen for ctrl+c")?;

    tracing::info!("shutting down");
    Ok(())
}
