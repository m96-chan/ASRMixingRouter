pub mod config;
pub mod config_diff;
pub mod error;
pub mod tui_types;
pub mod types;

pub use config::AppConfig;
pub use config_diff::ConfigDiff;
pub use error::{AsrError, AudioError, ConfigError, DestinationError};
pub use tui_types::{InputState, InputStatus, OutputState, RouterState, UiCommand};
pub use types::{AudioChunk, RecognitionResult, TextMetadata};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_chunk_creation() {
        let chunk = AudioChunk {
            samples: vec![0.0, 0.5, -0.5, 1.0],
            sample_rate: 48000,
            channels: 1,
        };
        assert_eq!(chunk.samples.len(), 4);
        assert_eq!(chunk.sample_rate, 48000);
        assert_eq!(chunk.channels, 1);
    }

    #[test]
    fn test_recognition_result_fields() {
        let result = RecognitionResult {
            text: "hello world".to_string(),
            input_id: "mic1".to_string(),
            timestamp: 1.5,
            is_final: true,
        };
        assert_eq!(result.text, "hello world");
        assert_eq!(result.input_id, "mic1");
        assert_eq!(result.timestamp, 1.5);
        assert!(result.is_final);
    }

    #[test]
    fn test_text_metadata_fields() {
        let meta = TextMetadata {
            input_id: "radio1".to_string(),
            prefix: "[R1] ".to_string(),
        };
        assert_eq!(meta.input_id, "radio1");
        assert_eq!(meta.prefix, "[R1] ");
    }
}
