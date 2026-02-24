use crate::config::AppConfig;

/// Describes runtime-safe changes between two configs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfigDiff {
    pub volume_changes: Vec<(String, f32)>,
    pub mute_changes: Vec<(String, bool)>,
    pub play_mixed_change: Option<bool>,
    pub non_reloadable: Vec<String>,
}

impl ConfigDiff {
    /// Compare two configs and return the diff.
    /// Reloadable: volume, mute, play_mixed_input.
    /// Non-reloadable: device changes, sample_rate, buffer_size, ASR engine — logged as warnings.
    pub fn diff(old: &AppConfig, new: &AppConfig) -> Self {
        let mut result = Self::default();

        // Check non-reloadable general settings
        if old.general.sample_rate != new.general.sample_rate {
            result.non_reloadable.push(format!(
                "sample_rate changed ({} → {}), requires restart",
                old.general.sample_rate, new.general.sample_rate
            ));
        }
        if old.general.buffer_size != new.general.buffer_size {
            result.non_reloadable.push(format!(
                "buffer_size changed ({} → {}), requires restart",
                old.general.buffer_size, new.general.buffer_size
            ));
        }

        // Check output device change (non-reloadable)
        if old.output.device_name != new.output.device_name {
            result.non_reloadable.push(format!(
                "output device changed ('{}' → '{}'), requires restart",
                old.output.device_name, new.output.device_name
            ));
        }

        // Check play_mixed_input (reloadable)
        if old.output.play_mixed_input != new.output.play_mixed_input {
            result.play_mixed_change = Some(new.output.play_mixed_input);
        }

        // Check per-input changes
        for new_input in &new.input {
            if let Some(old_input) = old.input.iter().find(|i| i.id == new_input.id) {
                // Volume change (reloadable)
                if (old_input.volume - new_input.volume).abs() > f32::EPSILON {
                    result
                        .volume_changes
                        .push((new_input.id.clone(), new_input.volume));
                }
                // Mute change (reloadable)
                if old_input.muted != new_input.muted {
                    result
                        .mute_changes
                        .push((new_input.id.clone(), new_input.muted));
                }
                // Device name change (non-reloadable)
                if old_input.device_name != new_input.device_name {
                    result.non_reloadable.push(format!(
                        "input '{}' device changed ('{}' → '{}'), requires restart",
                        new_input.id, old_input.device_name, new_input.device_name
                    ));
                }
            }
        }

        // Check ASR engine change (non-reloadable)
        match (&old.asr, &new.asr) {
            (Some(old_asr), Some(new_asr)) if old_asr.engine != new_asr.engine => {
                result.non_reloadable.push(format!(
                    "ASR engine changed ('{}' → '{}'), requires restart",
                    old_asr.engine, new_asr.engine
                ));
            }
            _ => {}
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> AppConfig {
        AppConfig::from_toml_str(
            r#"
[output]
device_name = "speakers"
play_mixed_input = true

[[input]]
id = "mic1"
device_name = "USB Mic"
volume = 0.8
muted = false
"#,
        )
        .unwrap()
    }

    #[test]
    fn test_config_diff_volume_change() {
        let old = base_config();
        let new = AppConfig::from_toml_str(
            r#"
[output]
device_name = "speakers"
play_mixed_input = true

[[input]]
id = "mic1"
device_name = "USB Mic"
volume = 0.5
muted = false
"#,
        )
        .unwrap();

        let diff = ConfigDiff::diff(&old, &new);
        assert_eq!(diff.volume_changes, vec![("mic1".to_string(), 0.5)]);
        assert!(diff.mute_changes.is_empty());
        assert!(diff.non_reloadable.is_empty());
    }

    #[test]
    fn test_config_diff_mute_change() {
        let old = base_config();
        let new = AppConfig::from_toml_str(
            r#"
[output]
device_name = "speakers"
play_mixed_input = true

[[input]]
id = "mic1"
device_name = "USB Mic"
volume = 0.8
muted = true
"#,
        )
        .unwrap();

        let diff = ConfigDiff::diff(&old, &new);
        assert!(diff.volume_changes.is_empty());
        assert_eq!(diff.mute_changes, vec![("mic1".to_string(), true)]);
    }

    #[test]
    fn test_config_diff_no_change() {
        let old = base_config();
        let new = base_config();
        let diff = ConfigDiff::diff(&old, &new);
        assert!(diff.volume_changes.is_empty());
        assert!(diff.mute_changes.is_empty());
        assert!(diff.play_mixed_change.is_none());
        assert!(diff.non_reloadable.is_empty());
    }

    #[test]
    fn test_config_diff_ignores_device_change() {
        let old = base_config();
        let new = AppConfig::from_toml_str(
            r#"
[output]
device_name = "speakers"
play_mixed_input = true

[[input]]
id = "mic1"
device_name = "New Device"
volume = 0.8
muted = false
"#,
        )
        .unwrap();

        let diff = ConfigDiff::diff(&old, &new);
        assert!(diff.volume_changes.is_empty());
        assert_eq!(diff.non_reloadable.len(), 1);
        assert!(diff.non_reloadable[0].contains("device changed"));
    }

    #[test]
    fn test_config_diff_play_mixed_change() {
        let old = base_config();
        let new = AppConfig::from_toml_str(
            r#"
[output]
device_name = "speakers"
play_mixed_input = false

[[input]]
id = "mic1"
device_name = "USB Mic"
volume = 0.8
muted = false
"#,
        )
        .unwrap();

        let diff = ConfigDiff::diff(&old, &new);
        assert_eq!(diff.play_mixed_change, Some(false));
    }
}
