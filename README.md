# voxmux

A Rust application that mixes multiple audio input devices for speaker output while independently running ASR (Automatic Speech Recognition) on each input and routing the recognized text to multiple destinations (e.g., Discord). Controllable via a TUI during runtime. Supports macOS and Linux.

Designed for scenarios such as routing multiple radio receivers (e.g., license-free radios, digital simplex radios) connected to the PC via 3.5mm audio jacks, where each receiver's audio is captured as a separate input device.

## Data Flow

```
InputDevice1 ──┬──→ Mixer ──→ OutputDevice (Speaker)
InputDevice2 ──┤
InputDeviceN ──┘

InputDevice1 ──→ ASR Engine ──→ Text ──→ [Destination1(prefix), Destination2(prefix)]
InputDevice2 ──→ ASR Engine ──→ Text ──→ [Destination3(prefix)]
```

Each input device feeds into a shared mixer for speaker output. Simultaneously, each input is tapped and sent to an ASR engine. The recognized text is then routed to one or more configured destinations with optional per-destination prefixes.

## Project Structure

The project is organized as a Cargo workspace:

```
voxmux/
├── Cargo.toml              # Workspace root
├── config.example.toml
├── crates/
│   ├── voxmux-core/        # Common traits, types, config, errors
│   ├── voxmux-audio/       # Audio capture, mixer, output (cpal + ringbuf)
│   ├── voxmux-engine/      # ASR plugin host + whisper integration
│   ├── voxmux-destination/ # Destination plugin host + discord integration
│   ├── voxmux-tui/         # TUI (ratatui + crossterm)
│   └── voxmux-router/      # Pipeline construction & routing
└── src/main.rs             # Binary entry point
```

### Crate Responsibilities

| Crate | Description |
|-------|-------------|
| `voxmux-core` | Shared traits, config schema (TOML), error types, and audio primitives (`AudioChunk`, `RecognitionResult`, etc.) |
| `voxmux-audio` | Device enumeration, audio capture via cpal, lock-free SPSC ring buffers (ringbuf), N-to-1 mixer, and speaker output |
| `voxmux-engine` | `AsrEngine` trait, plugin registry, and whisper-rs integration (feature-gated) |
| `voxmux-destination` | `Destination` trait, plugin registry, and Discord integration via serenity (feature-gated) |
| `voxmux-tui` | Terminal UI with ratatui + crossterm — dashboard, input/output controls, and log viewer |
| `voxmux-router` | Orchestrates the full pipeline: config loading, device setup, ASR dispatch, text routing, and TUI communication |

## Core Traits

### AsrEngine

```rust
#[async_trait]
pub trait AsrEngine: Send + Sync {
    fn name(&self) -> &str;
    async fn initialize(&mut self, config: toml::Value) -> Result<(), AsrError>;
    async fn feed_audio(&self, chunk: AudioChunk) -> Result<(), AsrError>;
    fn set_result_sender(&mut self, sender: mpsc::UnboundedSender<RecognitionResult>);
    async fn shutdown(&self) -> Result<(), AsrError>;
}
```

### Destination

```rust
#[async_trait]
pub trait Destination: Send + Sync {
    fn name(&self) -> &str;
    async fn initialize(&mut self, config: toml::Value) -> Result<(), DestinationError>;
    async fn send_text(&self, text: &str, metadata: &TextMetadata) -> Result<(), DestinationError>;
    fn is_healthy(&self) -> bool;
    async fn shutdown(&self) -> Result<(), DestinationError>;
}
```

### Plugin System

- **Phase 1**: Compile-time registration via `PluginRegistry` with feature flags
- **Future**: Dynamic loading via `libloading` (feature-gated)

## Audio Pipeline

```
cpal input callback → SPSC ring buffer (lock-free, per device)
                          ↓
Mixer thread: read all input ring buffers → apply gain/mute → sum → output ring buffer
                          ↓
cpal output callback ← output ring buffer

CaptureNode → mpsc channel → ASR task (parallel tap, does not block the mixer)
```

- **cpal** handles cross-platform audio I/O
- **ringbuf** provides lock-free SPSC ring buffers between the real-time audio callbacks and processing threads
- Volume and mute are controlled via atomics for lock-free, real-time-safe adjustment

## Configuration

Configuration is defined in TOML. Environment variables can be interpolated with `${VAR_NAME}` syntax.

```toml
[general]
log_level = "info"
sample_rate = 48000
buffer_size = 1024

[output]
device_name = "default"
play_mixed_input = true

[asr]
engine = "whisper"

[asr.whisper]
model_path = "./models/ggml-base.bin"
language = "ja"

[[input]]
id = "mic_main"
device_name = "MacBook Pro Microphone"
enabled = true
volume = 1.0
muted = false

[[input.destinations]]
plugin = "discord"
prefix = "[Main] "
channel_id = 123456789

[destinations.discord]
token = "${DISCORD_TOKEN}"
guild_id = 987654321
```

## TUI

The TUI provides four tabs:

| Tab | Contents |
|-----|----------|
| **Dashboard** | Overall status, VU meters, latest recognized text |
| **Inputs** | Per-device volume, mute, and enable controls |
| **Outputs** | Speaker output settings, play-mixed-input toggle |
| **Logs** | Scrollable tracing log viewer |

Communication between the TUI and the router:

- **TUI → Router**: `UiCommand` sent via mpsc channel (volume changes, mute toggles, etc.)
- **Router → TUI**: `RouterState` broadcast via `watch::Sender` for real-time state synchronization

## Dependencies

| Crate | Purpose |
|-------|---------|
| [cpal](https://crates.io/crates/cpal) | Cross-platform audio I/O |
| [ringbuf](https://crates.io/crates/ringbuf) | Lock-free SPSC ring buffer |
| [ratatui](https://crates.io/crates/ratatui) + [crossterm](https://crates.io/crates/crossterm) | Terminal UI |
| [tokio](https://crates.io/crates/tokio) | Async runtime |
| [serde](https://crates.io/crates/serde) + [toml](https://crates.io/crates/toml) | Configuration |
| [thiserror](https://crates.io/crates/thiserror) / [anyhow](https://crates.io/crates/anyhow) | Error handling |
| [tracing](https://crates.io/crates/tracing) | Logging |
| [whisper-rs](https://crates.io/crates/whisper-rs) | Whisper ASR engine (feature-gated) |
| [serenity](https://crates.io/crates/serenity) | Discord bot (feature-gated) |
| [async-trait](https://crates.io/crates/async-trait) | Async trait support |
| [clap](https://crates.io/crates/clap) | CLI argument parsing |

## Roadmap

### Phase 1: Foundation
- Cargo workspace and crate scaffolding
- Core types: config schema, error types, audio primitives
- Single input → speaker output passthrough

### Phase 2: Multi-Input Mixing
- N-to-1 mixer with gain/mute per input
- Concurrent multi-device capture
- Real-time volume/mute control via atomics

### Phase 3: ASR Integration
- `AsrEngine` trait and plugin registry
- ASR tap on each capture node
- whisper-rs engine integration

### Phase 4: Destination Routing
- `Destination` trait and plugin registry
- Config-driven routing table
- File destination (for testing)
- Discord destination (serenity)
- Per-destination prefix support

### Phase 5: TUI
- Event loop and application state
- Four-tab layout (Dashboard / Inputs / Outputs / Logs)
- VU meters and volume sliders
- Bidirectional TUI ↔ Router communication

### Phase 6: Polish
- Robust error handling and recovery
- Device disconnect/reconnect handling
- Config hot-reload
- CI for macOS and Linux

## License

This project is licensed under the [GNU General Public License v3.0](LICENSE).
