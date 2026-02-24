use crate::error::ConfigError;
use regex::Regex;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,

    #[serde(default)]
    pub output: OutputConfig,

    #[serde(default)]
    pub input: Vec<InputConfig>,

    #[serde(default)]
    pub asr: Option<AsrConfig>,

    #[serde(default)]
    pub destinations: Option<toml::Value>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GeneralConfig {
    #[serde(default = "default_log_level")]
    pub log_level: String,

    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,

    #[serde(default = "default_buffer_size")]
    pub buffer_size: u32,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            sample_rate: default_sample_rate(),
            buffer_size: default_buffer_size(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct OutputConfig {
    #[serde(default = "default_device_name")]
    pub device_name: String,

    #[serde(default = "default_true")]
    pub play_mixed_input: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            device_name: default_device_name(),
            play_mixed_input: default_true(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct InputConfig {
    pub id: String,

    #[serde(default = "default_device_name")]
    pub device_name: String,

    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_volume")]
    pub volume: f32,

    #[serde(default)]
    pub muted: bool,

    #[serde(default)]
    pub destinations: Vec<DestinationRouteConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DestinationRouteConfig {
    pub plugin: String,

    #[serde(default)]
    pub prefix: String,

    #[serde(flatten)]
    pub extra: toml::Value,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AsrConfig {
    pub engine: String,

    #[serde(default)]
    pub whisper: Option<WhisperConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WhisperConfig {
    pub model_path: String,

    #[serde(default = "default_language")]
    pub language: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_sample_rate() -> u32 {
    48000
}

fn default_buffer_size() -> u32 {
    1024
}

fn default_device_name() -> String {
    "default".to_string()
}

fn default_true() -> bool {
    true
}

fn default_volume() -> f32 {
    1.0
}

fn default_language() -> String {
    "ja".to_string()
}

/// Interpolate `${VAR}` patterns with environment variable values.
fn interpolate_env_vars(input: &str) -> Result<String, ConfigError> {
    let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
    let mut result = input.to_string();
    let mut errors = Vec::new();

    for cap in re.captures_iter(input) {
        let var_name = &cap[1];
        match std::env::var(var_name) {
            Ok(val) => {
                result = result.replace(&cap[0], &val);
            }
            Err(_) => {
                errors.push(var_name.to_string());
            }
        }
    }

    if let Some(first_missing) = errors.into_iter().next() {
        return Err(ConfigError::EnvVarNotFound(first_missing));
    }

    Ok(result)
}

impl AppConfig {
    /// Load configuration from a TOML file, with environment variable interpolation.
    pub fn load_from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let interpolated = interpolate_env_vars(&content)?;
        let config: AppConfig = toml::from_str(&interpolated)?;
        Ok(config)
    }

    /// Parse configuration from a TOML string (for testing).
    pub fn from_toml_str(s: &str) -> Result<Self, ConfigError> {
        let interpolated = interpolate_env_vars(s)?;
        let config: AppConfig = toml::from_str(&interpolated)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_parse_valid_toml() {
        let toml_str = r#"
[general]
log_level = "debug"
sample_rate = 44100
buffer_size = 512

[output]
device_name = "speakers"
play_mixed_input = true

[[input]]
id = "mic1"
device_name = "USB Microphone"
enabled = true
volume = 0.8
muted = false

[[input.destinations]]
plugin = "discord"
prefix = "[Mic1] "
channel_id = 123456789
"#;
        let config = AppConfig::from_toml_str(toml_str).unwrap();
        assert_eq!(config.general.log_level, "debug");
        assert_eq!(config.general.sample_rate, 44100);
        assert_eq!(config.general.buffer_size, 512);
        assert_eq!(config.output.device_name, "speakers");
        assert_eq!(config.input.len(), 1);
        assert_eq!(config.input[0].id, "mic1");
        assert_eq!(config.input[0].volume, 0.8);
        assert_eq!(config.input[0].destinations.len(), 1);
        assert_eq!(config.input[0].destinations[0].plugin, "discord");
        assert_eq!(config.input[0].destinations[0].prefix, "[Mic1] ");
    }

    #[test]
    fn test_config_parse_minimal_toml() {
        let toml_str = r#"
[[input]]
id = "mic1"
"#;
        let config = AppConfig::from_toml_str(toml_str).unwrap();
        assert_eq!(config.general.log_level, "info");
        assert_eq!(config.general.sample_rate, 48000);
        assert_eq!(config.general.buffer_size, 1024);
        assert_eq!(config.output.device_name, "default");
        assert!(config.output.play_mixed_input);
        assert_eq!(config.input[0].device_name, "default");
        assert!(config.input[0].enabled);
        assert_eq!(config.input[0].volume, 1.0);
        assert!(!config.input[0].muted);
    }

    #[test]
    fn test_config_env_var_interpolation() {
        std::env::set_var("ASR_TEST_TOKEN", "secret123");
        let toml_str = r#"
[general]
log_level = "${ASR_TEST_TOKEN}"
"#;
        let config = AppConfig::from_toml_str(toml_str).unwrap();
        assert_eq!(config.general.log_level, "secret123");
        std::env::remove_var("ASR_TEST_TOKEN");
    }

    #[test]
    fn test_config_missing_env_var_error() {
        let toml_str = r#"
[general]
log_level = "${DEFINITELY_DOES_NOT_EXIST_12345}"
"#;
        let result = AppConfig::from_toml_str(toml_str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("DEFINITELY_DOES_NOT_EXIST_12345"),
        );
    }

    #[test]
    fn test_config_invalid_toml_error() {
        let toml_str = "this is not valid toml [[[";
        let result = AppConfig::from_toml_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_default_values() {
        let config = AppConfig::from_toml_str("").unwrap();
        assert_eq!(config.general.log_level, "info");
        assert_eq!(config.general.sample_rate, 48000);
        assert_eq!(config.general.buffer_size, 1024);
        assert_eq!(config.output.device_name, "default");
        assert!(config.output.play_mixed_input);
        assert!(config.input.is_empty());
        assert!(config.asr.is_none());
    }

    #[test]
    fn test_config_load_from_file() {
        let dir = std::env::temp_dir().join("asr_test_config");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.toml");
        std::fs::write(
            &path,
            r#"
[general]
log_level = "warn"
sample_rate = 16000

[[input]]
id = "test_mic"
"#,
        )
        .unwrap();

        let config = AppConfig::load_from_file(&path).unwrap();
        assert_eq!(config.general.log_level, "warn");
        assert_eq!(config.general.sample_rate, 16000);
        assert_eq!(config.input[0].id, "test_mic");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_config_load_from_file_not_found() {
        let result = AppConfig::load_from_file(std::path::Path::new("/nonexistent/path.toml"));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("failed to read config file"),
        );
    }

    #[test]
    fn test_config_multiple_inputs() {
        let toml_str = r#"
[[input]]
id = "radio1"
device_name = "USB Audio #1"
volume = 0.5

[[input]]
id = "radio2"
device_name = "USB Audio #2"
volume = 0.8
muted = true
"#;
        let config = AppConfig::from_toml_str(toml_str).unwrap();
        assert_eq!(config.input.len(), 2);
        assert_eq!(config.input[0].id, "radio1");
        assert_eq!(config.input[0].volume, 0.5);
        assert!(!config.input[0].muted);
        assert_eq!(config.input[1].id, "radio2");
        assert_eq!(config.input[1].volume, 0.8);
        assert!(config.input[1].muted);
    }

    #[test]
    fn test_config_asr_and_whisper_section() {
        let toml_str = r#"
[asr]
engine = "whisper"

[asr.whisper]
model_path = "./models/ggml-base.bin"
language = "en"
"#;
        let config = AppConfig::from_toml_str(toml_str).unwrap();
        let asr = config.asr.unwrap();
        assert_eq!(asr.engine, "whisper");
        let whisper = asr.whisper.unwrap();
        assert_eq!(whisper.model_path, "./models/ggml-base.bin");
        assert_eq!(whisper.language, "en");
    }

    #[test]
    fn test_config_whisper_default_language() {
        let toml_str = r#"
[asr]
engine = "whisper"

[asr.whisper]
model_path = "./models/ggml-base.bin"
"#;
        let config = AppConfig::from_toml_str(toml_str).unwrap();
        let whisper = config.asr.unwrap().whisper.unwrap();
        assert_eq!(whisper.language, "ja");
    }

    #[test]
    fn test_config_destination_route_extra_fields() {
        let toml_str = r#"
[[input]]
id = "mic1"

[[input.destinations]]
plugin = "discord"
prefix = "[Mic1] "
channel_id = 123456789
"#;
        let config = AppConfig::from_toml_str(toml_str).unwrap();
        let dest = &config.input[0].destinations[0];
        assert_eq!(dest.plugin, "discord");
        assert_eq!(dest.prefix, "[Mic1] ");
        // Verify extra fields are captured via #[serde(flatten)]
        assert_eq!(dest.extra.get("channel_id").unwrap().as_integer(), Some(123456789));
    }
}
