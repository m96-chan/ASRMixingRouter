use anyhow::{Context, Result};
use clap::Parser;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "voxmux", about = "Audio mixing router with ASR")]
struct Cli {
    /// Path to the configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let config = voxmux_core::AppConfig::load_from_file(&cli.config)
        .with_context(|| format!("failed to load config from {:?}", cli.config))?;

    // Set up TUI log buffer and layered tracing subscriber
    let log_buffer = Arc::new(Mutex::new(VecDeque::<String>::new()));
    let tui_log_layer = voxmux_tui::TuiLogLayer::new(Arc::clone(&log_buffer), 1000);

    let env_filter = EnvFilter::try_new(&config.general.log_level)
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = tracing_subscriber::Registry::default()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(false),
        )
        .with(tui_log_layer);

    tracing::subscriber::set_global_default(subscriber)
        .context("failed to set tracing subscriber")?;

    tracing::info!("voxmux starting");

    let device_manager = voxmux_audio::DeviceManager::new();

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
    let (out_producer, out_consumer) = voxmux_audio::create_ring_buffer(ring_capacity);

    // Create mixer with output producer
    let mut mixer = voxmux_audio::Mixer::new(out_producer, buffer_size as usize);

    // Create a CaptureNode + ring buffer for each enabled input
    let enabled_inputs: Vec<_> = config.input.iter().filter(|i| i.enabled).collect();
    if enabled_inputs.is_empty() {
        tracing::warn!("no enabled inputs configured");
    }

    // Set up ASR if configured
    let mut asr_host = None;
    let mut dest_host_handle: Option<voxmux_destination::DestinationHost> = None;
    let mut tap_senders = std::collections::HashMap::new();

    if let Some(ref asr_config) = config.asr {
        let registry = voxmux_engine::PluginRegistry::new();
        let mut host = voxmux_engine::AsrHost::new();

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

        // Set up destination routing for ASR results
        if let Some(result_rx) = host.take_result_receiver() {
            let has_destinations = enabled_inputs
                .iter()
                .any(|i| !i.destinations.is_empty());

            if has_destinations {
                let mut dest_host = voxmux_destination::DestinationHost::new(result_rx);

                for input_cfg in &enabled_inputs {
                    for route_cfg in &input_cfg.destinations {
                        // Merge global destination config with per-route extra
                        let mut merged = match config.destinations {
                            Some(ref dests) => dests
                                .get(&route_cfg.plugin)
                                .cloned()
                                .unwrap_or_else(|| toml::Value::Table(Default::default())),
                            None => toml::Value::Table(Default::default()),
                        };

                        // Overlay per-route extra fields
                        if let (Some(base), Some(extra)) =
                            (merged.as_table_mut(), route_cfg.extra.as_table())
                        {
                            for (k, v) in extra {
                                base.insert(k.clone(), v.clone());
                            }
                        }

                        dest_host
                            .add_route(
                                &input_cfg.id,
                                &route_cfg.plugin,
                                &route_cfg.prefix,
                                merged,
                            )
                            .await
                            .with_context(|| {
                                format!(
                                    "failed to add destination route '{}' for input '{}'",
                                    route_cfg.plugin, input_cfg.id
                                )
                            })?;

                        tracing::info!(
                            "routed input '{}' → destination '{}' (prefix: {:?})",
                            input_cfg.id,
                            route_cfg.plugin,
                            route_cfg.prefix,
                        );
                    }
                }

                dest_host.start();
                dest_host_handle = Some(dest_host);
            } else {
                // Fallback: log ASR results when no destinations configured
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
        }

        host.start();
        tracing::info!("ASR engine '{}' active", asr_config.engine);
        asr_host = Some(host);
    }

    // Keep capture nodes alive for the duration of the program
    let mut _captures = Vec::new();
    let mut input_handles = Vec::new();

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

        let (in_prod, in_cons) = voxmux_audio::create_ring_buffer(ring_capacity);

        let handle = mixer.add_input(&input_cfg.id, in_cons, input_cfg.volume, input_cfg.muted);
        input_handles.push(handle);

        let asr_tap = tap_senders.remove(&input_cfg.id);

        let capture = voxmux_audio::CaptureNode::new(
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
    let _output = voxmux_audio::OutputNode::new(
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

    // Set up TUI communication channels
    let (state_tx, state_rx) =
        tokio::sync::watch::channel(voxmux_core::RouterState::default());
    let (cmd_tx, mut cmd_rx) =
        tokio::sync::mpsc::unbounded_channel::<voxmux_core::UiCommand>();

    // Capture config data needed by the state broadcast task
    let input_configs: Vec<_> = enabled_inputs
        .iter()
        .map(|i| (i.id.clone(), i.device_name.clone(), i.enabled))
        .collect();
    let output_device_name = config.output.device_name.clone();
    let play_mixed_input = config.output.play_mixed_input;

    // Spawn state broadcast task (~30Hz)
    let broadcast_handles = input_handles.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(33));
        loop {
            interval.tick().await;
            let inputs: Vec<voxmux_core::InputState> = broadcast_handles
                .iter()
                .zip(input_configs.iter())
                .map(|(handle, (id, device_name, enabled))| voxmux_core::InputState {
                    id: id.clone(),
                    device_name: device_name.clone(),
                    enabled: *enabled,
                    volume: handle.volume(),
                    muted: handle.is_muted(),
                    peak_level: 0.0, // proper computation deferred to P6
                })
                .collect();

            let state = voxmux_core::RouterState {
                inputs,
                output: voxmux_core::OutputState {
                    device_name: output_device_name.clone(),
                    play_mixed_input,
                },
                latest_recognitions: Vec::new(), // populated when ASR integration deepens
                is_running: true,
            };

            if state_tx.send(state).is_err() {
                break; // TUI closed
            }
        }
    });

    // Spawn command handler task
    let cmd_handles = input_handles.clone();
    tokio::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                voxmux_core::UiCommand::SetVolume { input_id, volume } => {
                    if let Some(h) = cmd_handles.iter().find(|h| h.id() == input_id) {
                        h.set_volume(volume);
                    }
                }
                voxmux_core::UiCommand::SetMuted { input_id, muted } => {
                    if let Some(h) = cmd_handles.iter().find(|h| h.id() == input_id) {
                        h.set_muted(muted);
                    }
                }
                voxmux_core::UiCommand::SetEnabled { .. } => {
                    // No-op in P5 — requires capture node stop/start
                }
                voxmux_core::UiCommand::SetPlayMixedInput(_) => {
                    // No-op in P5 — output control deferred
                }
                voxmux_core::UiCommand::Quit => {
                    break;
                }
            }
        }
    });

    tracing::info!("TUI active — press 'q' to quit");

    // Run TUI (blocks until user quits)
    voxmux_tui::run(state_rx, cmd_tx, log_buffer)
        .await
        .context("TUI error")?;

    tracing::info!("shutting down");
    mixer_handle.stop();

    if let Some(mut host) = asr_host {
        host.shutdown().await;
    }

    if let Some(mut dest_host) = dest_host_handle {
        dest_host.shutdown().await;
    }

    Ok(())
}
