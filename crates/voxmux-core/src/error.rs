use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    FileRead(#[from] std::io::Error),

    #[error("failed to parse TOML: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("environment variable not found: {0}")]
    EnvVarNotFound(String),
}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("device not found: {0}")]
    DeviceNotFound(String),

    #[error("failed to enumerate devices: {0}")]
    DeviceEnumeration(String),

    #[error("failed to build stream: {0}")]
    StreamBuild(String),

    #[error("stream error: {0}")]
    StreamError(String),
}

#[derive(Debug, Error)]
pub enum AsrError {
    #[error("ASR initialization failed: {0}")]
    InitializationFailed(String),

    #[error("ASR processing failed: {0}")]
    ProcessingFailed(String),

    #[error("ASR engine not found: {0}")]
    EngineNotFound(String),
}

#[derive(Debug, Error)]
pub enum DestinationError {
    #[error("destination initialization failed: {0}")]
    InitializationFailed(String),

    #[error("failed to send text: {0}")]
    SendFailed(String),

    #[error("destination not found: {0}")]
    NotFound(String),

    #[error("destination connection lost: {0}")]
    ConnectionLost(String),
}
