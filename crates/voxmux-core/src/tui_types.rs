/// Health status for an input or output device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputStatus {
    #[default]
    Ok,
    Error,
    Disabled,
}

/// State of a single audio input, for TUI display.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InputState {
    pub id: String,
    pub device_name: String,
    pub enabled: bool,
    pub volume: f32,
    pub muted: bool,
    pub peak_level: f32,
    pub status: InputStatus,
}

/// State of the audio output, for TUI display.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputState {
    pub device_name: String,
    pub play_mixed_input: bool,
}

impl Default for OutputState {
    fn default() -> Self {
        Self {
            device_name: "default".to_string(),
            play_mixed_input: true,
        }
    }
}

/// Aggregate router state broadcast to the TUI via watch channel.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RouterState {
    pub inputs: Vec<InputState>,
    pub output: OutputState,
    pub latest_recognitions: Vec<String>,
    pub warnings: Vec<String>,
    pub is_running: bool,
}

/// Commands sent from TUI → main via mpsc channel.
#[derive(Debug, Clone, PartialEq)]
pub enum UiCommand {
    SetVolume { input_id: String, volume: f32 },
    SetMuted { input_id: String, muted: bool },
    SetEnabled { input_id: String, enabled: bool },
    SetPlayMixedInput(bool),
    Quit,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_state_default() {
        let state = RouterState::default();
        assert!(state.inputs.is_empty());
        assert!(!state.is_running);
        assert!(state.latest_recognitions.is_empty());
        assert_eq!(state.output, OutputState::default());
    }

    #[test]
    fn test_input_state_default() {
        let input = InputState::default();
        assert_eq!(input.volume, 0.0);
        assert!(!input.enabled);
        assert!(!input.muted);
        assert_eq!(input.peak_level, 0.0);
        assert!(input.id.is_empty());
        assert!(input.device_name.is_empty());
        assert_eq!(input.status, InputStatus::Ok);
    }

    #[test]
    fn test_input_status_default_ok() {
        assert_eq!(InputStatus::default(), InputStatus::Ok);
    }

    #[test]
    fn test_input_state_has_status() {
        let input = InputState::default();
        assert_eq!(input.status, InputStatus::Ok);
    }

    #[test]
    fn test_router_state_has_warnings() {
        let state = RouterState::default();
        assert!(state.warnings.is_empty());
    }

    #[test]
    fn test_ui_command_clone_eq() {
        let cmd = UiCommand::SetVolume {
            input_id: "mic1".to_string(),
            volume: 0.75,
        };
        let cloned = cmd.clone();
        assert_eq!(cmd, cloned);
    }

    #[test]
    fn test_router_state_is_clone() {
        let state = RouterState {
            inputs: vec![InputState {
                id: "mic1".to_string(),
                device_name: "USB Mic".to_string(),
                enabled: true,
                volume: 0.8,
                muted: false,
                peak_level: 0.5,
                status: InputStatus::Ok,
            }],
            output: OutputState {
                device_name: "speakers".to_string(),
                play_mixed_input: true,
            },
            latest_recognitions: vec!["hello".to_string()],
            warnings: Vec::new(),
            is_running: true,
        };
        let cloned = state.clone();
        assert_eq!(state, cloned);
    }
}
