use anyhow::{Context, Result};
use clap::Parser;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::EnvFilter;

const RECOGNITION_BUFFER_CAPACITY: usize = 50;

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

    // Recognition buffer for TUI display (shared across ASR + broadcast tasks)
    let recognition_buf = Arc::new(Mutex::new(VecDeque::<String>::new()));

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
                // Create a forwarder channel: result_rx → forwarder → dest_host
                let (fwd_tx, fwd_rx) =
                    tokio::sync::mpsc::unbounded_channel::<voxmux_core::RecognitionResult>();

                let mut dest_host = voxmux_destination::DestinationHost::new(fwd_rx);

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

                // Forwarder task: copies to recognition buffer + forwards to DestinationHost
                let fwd_recog_buf = Arc::clone(&recognition_buf);
                tokio::spawn(async move {
                    let mut rx = result_rx;
                    while let Some(result) = rx.recv().await {
                        if result.is_final {
                            let text =
                                format!("[{}] {}", result.input_id, result.text);
                            push_recognition(&fwd_recog_buf, text);
                        }
                        let _ = fwd_tx.send(result);
                    }
                });
            } else {
                // Fallback: log ASR results + push to recognition buffer
                let fallback_recog_buf = Arc::clone(&recognition_buf);
                tokio::spawn(async move {
                    let mut rx = result_rx;
                    while let Some(result) = rx.recv().await {
                        tracing::info!(
                            input_id = %result.input_id,
                            is_final = result.is_final,
                            "ASR: {}",
                            result.text,
                        );
                        if result.is_final {
                            let text =
                                format!("[{}] {}", result.input_id, result.text);
                            push_recognition(&fallback_recog_buf, text);
                        }
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
    let mut capture_handles = Vec::new();

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

        let (capture, capture_handle) = voxmux_audio::CaptureNode::new(
            &input_device,
            in_prod,
            sample_rate,
            channels,
            buffer_size,
            asr_tap,
            &input_cfg.id,
        )
        .with_context(|| format!("failed to create capture node for '{}'", input_cfg.id))?;

        _captures.push(capture);
        capture_handles.push(capture_handle);
    }

    // Start output node
    let (_output, output_handle) = voxmux_audio::OutputNode::new(
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
        .map(|i| (i.id.clone(), i.device_name.clone()))
        .collect();
    let output_device_name = config.output.device_name.clone();

    // Spawn state broadcast task (~30Hz)
    let broadcast_handles = input_handles.clone();
    let broadcast_capture_handles = capture_handles.clone();
    let broadcast_output_handle = output_handle.clone();
    let broadcast_recog_buf = Arc::clone(&recognition_buf);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(33));
        loop {
            interval.tick().await;
            let inputs: Vec<voxmux_core::InputState> = broadcast_handles
                .iter()
                .zip(input_configs.iter())
                .zip(broadcast_capture_handles.iter())
                .map(|((handle, (id, device_name)), cap_handle)| {
                    let status = if !cap_handle.is_enabled() {
                        voxmux_core::InputStatus::Disabled
                    } else {
                        cap_handle.status()
                    };
                    voxmux_core::InputState {
                        id: id.clone(),
                        device_name: device_name.clone(),
                        enabled: cap_handle.is_enabled(),
                        volume: handle.volume(),
                        muted: handle.is_muted(),
                        peak_level: handle.peak_level(),
                        status,
                    }
                })
                .collect();

            // Collect warnings from unhealthy devices
            let mut warnings = Vec::new();
            for (cap_handle, (id, _)) in
                broadcast_capture_handles.iter().zip(input_configs.iter())
            {
                if cap_handle.status() == voxmux_core::InputStatus::Error {
                    warnings.push(format!("Input '{}' stream error", id));
                }
            }
            if broadcast_output_handle.status() == voxmux_core::InputStatus::Error {
                warnings.push("Output stream error".to_string());
            }

            let recognitions = broadcast_recog_buf
                .lock()
                .map(|q| q.iter().cloned().collect())
                .unwrap_or_default();

            let state = voxmux_core::RouterState {
                inputs,
                output: voxmux_core::OutputState {
                    device_name: output_device_name.clone(),
                    play_mixed_input: broadcast_output_handle.is_playing(),
                },
                latest_recognitions: recognitions,
                warnings,
                is_running: true,
            };

            if state_tx.send(state).is_err() {
                break; // TUI closed
            }
        }
    });

    // Spawn command handler task
    let cmd_handles = input_handles.clone();
    let cmd_capture_handles = capture_handles.clone();
    let cmd_output_handle = output_handle.clone();
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
                voxmux_core::UiCommand::SetEnabled { input_id, enabled } => {
                    if let Some(h) =
                        cmd_capture_handles.iter().find(|h| h.id() == input_id)
                    {
                        h.set_enabled(enabled);
                    }
                }
                voxmux_core::UiCommand::SetPlayMixedInput(play) => {
                    cmd_output_handle.set_playing(play);
                }
                voxmux_core::UiCommand::Quit => {
                    break;
                }
            }
        }
    });

    // Spawn config hot-reload watcher
    let config_path = cli.config.clone();
    let reload_input_handles = input_handles.clone();
    let reload_capture_handles = capture_handles.clone();
    let reload_output_handle = output_handle.clone();
    let reload_config = config.clone();
    tokio::spawn(async move {
        use notify::{Event, RecursiveMode, Watcher};

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
        let mut watcher = match notify::recommended_watcher(move |res| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("config watcher failed to start: {}", e);
                return;
            }
        };

        if let Err(e) = watcher.watch(&config_path, RecursiveMode::NonRecursive) {
            tracing::warn!("failed to watch config file: {}", e);
            return;
        }

        tracing::info!("watching {:?} for changes", config_path);

        let mut current_config = reload_config;
        while let Some(event) = rx.recv().await {
            if !event.kind.is_modify() {
                continue;
            }
            // Small delay to let file writes complete
            tokio::time::sleep(Duration::from_millis(100)).await;

            let new_config = match voxmux_core::AppConfig::load_from_file(&config_path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("failed to reload config: {}", e);
                    continue;
                }
            };

            let diff = voxmux_core::ConfigDiff::diff(&current_config, &new_config);

            // Apply reloadable changes
            for (id, volume) in &diff.volume_changes {
                if let Some(h) = reload_input_handles.iter().find(|h| h.id() == id) {
                    h.set_volume(*volume);
                    tracing::info!("reloaded: input '{}' volume → {}", id, volume);
                }
            }
            for (id, muted) in &diff.mute_changes {
                if let Some(h) = reload_input_handles.iter().find(|h| h.id() == id) {
                    h.set_muted(*muted);
                    tracing::info!("reloaded: input '{}' muted → {}", id, muted);
                }
            }
            if let Some(play) = diff.play_mixed_change {
                reload_output_handle.set_playing(play);
                tracing::info!("reloaded: play_mixed_input → {}", play);
            }

            // Log non-reloadable changes as warnings
            for warning in &diff.non_reloadable {
                tracing::warn!("config change ignored: {}", warning);
            }

            // Apply enabled state from config
            for new_input in &new_config.input {
                if let Some(h) = reload_capture_handles.iter().find(|h| h.id() == new_input.id)
                {
                    if h.is_enabled() != new_input.enabled {
                        h.set_enabled(new_input.enabled);
                        tracing::info!(
                            "reloaded: input '{}' enabled → {}",
                            new_input.id,
                            new_input.enabled,
                        );
                    }
                }
            }

            current_config = new_config;
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

/// Push a recognition string into the bounded buffer, dropping oldest if full.
fn push_recognition(buf: &Arc<Mutex<VecDeque<String>>>, text: String) {
    if let Ok(mut q) = buf.lock() {
        if q.len() >= RECOGNITION_BUFFER_CAPACITY {
            q.pop_front();
        }
        q.push_back(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recognition_buffer_bounded() {
        let buf = Arc::new(Mutex::new(VecDeque::<String>::new()));
        for i in 0..55 {
            push_recognition(&buf, format!("msg{}", i));
        }
        let q = buf.lock().unwrap();
        assert_eq!(q.len(), RECOGNITION_BUFFER_CAPACITY);
        // Oldest 5 should have been dropped
        assert_eq!(q.front().unwrap(), "msg5");
        assert_eq!(q.back().unwrap(), "msg54");
    }

    #[tokio::test]
    async fn test_recognition_forwarder() {
        let buf = Arc::new(Mutex::new(VecDeque::<String>::new()));
        let buf_clone = Arc::clone(&buf);

        let (src_tx, mut src_rx) =
            tokio::sync::mpsc::unbounded_channel::<voxmux_core::RecognitionResult>();
        let (fwd_tx, mut fwd_rx) =
            tokio::sync::mpsc::unbounded_channel::<voxmux_core::RecognitionResult>();

        // Simulate forwarder task
        let handle = tokio::spawn(async move {
            while let Some(result) = src_rx.recv().await {
                let text = format!("[{}] {}", result.input_id, result.text);
                push_recognition(&buf_clone, text);
                let _ = fwd_tx.send(result);
            }
        });

        src_tx
            .send(voxmux_core::RecognitionResult {
                text: "hello world".to_string(),
                input_id: "mic1".to_string(),
                timestamp: 0.0,
                is_final: true,
            })
            .unwrap();

        drop(src_tx); // Close channel to let forwarder exit
        handle.await.unwrap();

        // Check buffer
        let q = buf.lock().unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0], "[mic1] hello world");
        drop(q);

        // Check forwarded
        let forwarded = fwd_rx.try_recv().unwrap();
        assert_eq!(forwarded.text, "hello world");
        assert_eq!(forwarded.input_id, "mic1");
    }
}
