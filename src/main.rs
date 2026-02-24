use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::time::Duration;
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

    // Output ring buffer: ~2 seconds of audio
    let ring_capacity = (sample_rate as usize) * (channels as usize) * 2;
    let (out_producer, out_consumer) = asr_audio::create_ring_buffer(ring_capacity);

    // Create mixer with output producer
    let mut mixer = asr_audio::Mixer::new(out_producer, buffer_size as usize);

    // Create a CaptureNode + ring buffer for each enabled input
    let enabled_inputs: Vec<_> = config.input.iter().filter(|i| i.enabled).collect();
    if enabled_inputs.is_empty() {
        tracing::warn!("no enabled inputs configured");
    }

    // Set up ASR if configured
    let mut asr_host = None;
    let mut tap_senders = std::collections::HashMap::new();

    if let Some(ref asr_config) = config.asr {
        let registry = asr_engine::PluginRegistry::new();
        let mut host = asr_engine::AsrHost::new();

        for input_cfg in &enabled_inputs {
            let engine_config = match asr_config.engine.as_str() {
                "whisper" => {
                    if let Some(ref whisper_cfg) = asr_config.whisper {
                        toml::Value::try_from(whisper_cfg)
                            .context("failed to serialize whisper config")?
                    } else {
                        toml::Value::Table(Default::default())
                    }
                }
                _ => toml::Value::Table(Default::default()),
            };

            let tap_tx = host
                .add_input(&input_cfg.id, &asr_config.engine, engine_config, &registry)
                .await
                .with_context(|| {
                    format!(
                        "failed to add ASR input '{}' with engine '{}'",
                        input_cfg.id, asr_config.engine
                    )
                })?;
            tap_senders.insert(input_cfg.id.clone(), tap_tx);
        }

        // Spawn result logging task
        if let Some(result_rx) = host.take_result_receiver() {
            tokio::spawn(async move {
                let mut rx = result_rx;
                while let Some(result) = rx.recv().await {
                    tracing::info!(
                        input_id = %result.input_id,
                        is_final = result.is_final,
                        "ASR: {}",
                        result.text,
                    );
                }
            });
        }

        host.start();
        tracing::info!("ASR engine '{}' active", asr_config.engine);
        asr_host = Some(host);
    }

    // Keep capture nodes alive for the duration of the program
    let mut _captures = Vec::new();

    for input_cfg in &enabled_inputs {
        tracing::info!(
            "adding input '{}' (device: {}, vol: {}, muted: {})",
            input_cfg.id,
            input_cfg.device_name,
            input_cfg.volume,
            input_cfg.muted,
        );

        let input_device = device_manager
            .get_input_device(&input_cfg.device_name)
            .with_context(|| {
                format!(
                    "failed to get input device '{}' for input '{}'",
                    input_cfg.device_name, input_cfg.id
                )
            })?;

        let (in_prod, in_cons) = asr_audio::create_ring_buffer(ring_capacity);

        let _handle = mixer.add_input(&input_cfg.id, in_cons, input_cfg.volume, input_cfg.muted);

        let asr_tap = tap_senders.remove(&input_cfg.id);

        let capture = asr_audio::CaptureNode::new(
            &input_device,
            in_prod,
            sample_rate,
            channels,
            buffer_size,
            asr_tap,
        )
        .with_context(|| format!("failed to create capture node for '{}'", input_cfg.id))?;

        _captures.push(capture);
    }

    // Start output node
    let _output = asr_audio::OutputNode::new(
        &output_device,
        out_consumer,
        sample_rate,
        channels,
        buffer_size,
    )
    .context("failed to create output node")?;

    tracing::info!(
        "mixing {} input(s) → output at {}Hz, {} ch, buffer={}",
        enabled_inputs.len(),
        sample_rate,
        channels,
        buffer_size,
    );

    // Start mixer thread (1ms poll interval)
    let mixer_handle = mixer.start(Duration::from_millis(1));

    tracing::info!("mixer active — press Ctrl+C to stop");

    tokio::signal::ctrl_c()
        .await
        .context("failed to listen for ctrl+c")?;

    tracing::info!("shutting down");
    mixer_handle.stop();

    if let Some(mut host) = asr_host {
        host.shutdown().await;
    }

    Ok(())
}
