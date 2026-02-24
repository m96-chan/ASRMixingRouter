use crate::dest_trait::Destination;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use voxmux_core::{DestinationError, TextMetadata};

pub struct FileDestination {
    output_path: Mutex<Option<PathBuf>>,
    send_count: AtomicUsize,
}

impl FileDestination {
    pub fn new() -> Self {
        Self {
            output_path: Mutex::new(None),
            send_count: AtomicUsize::new(0),
        }
    }

    pub fn send_count(&self) -> usize {
        self.send_count.load(Ordering::Relaxed)
    }
}

impl Default for FileDestination {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Destination for FileDestination {
    fn name(&self) -> &str {
        "file"
    }

    async fn initialize(&mut self, config: toml::Value) -> Result<(), DestinationError> {
        let path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                DestinationError::InitializationFailed("missing 'path' in config".to_string())
            })?;
        *self.output_path.lock().unwrap() = Some(PathBuf::from(path));
        Ok(())
    }

    async fn send_text(&self, text: &str, metadata: &TextMetadata) -> Result<(), DestinationError> {
        let guard = self.output_path.lock().unwrap();
        let path = guard.as_ref().ok_or_else(|| {
            DestinationError::SendFailed("not initialized".to_string())
        })?;

        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| DestinationError::SendFailed(e.to_string()))?;

        writeln!(file, "{}{}", metadata.prefix, text)
            .map_err(|e| DestinationError::SendFailed(e.to_string()))?;

        self.send_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn is_healthy(&self) -> bool {
        self.output_path.lock().unwrap().is_some()
    }

    async fn shutdown(&self) -> Result<(), DestinationError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_dest_name() {
        let dest = FileDestination::new();
        assert_eq!(dest.name(), "file");
    }

    #[tokio::test]
    async fn test_file_dest_initialize_sets_path() {
        let mut dest = FileDestination::new();
        let config = toml::Value::Table({
            let mut t = toml::map::Map::new();
            t.insert("path".to_string(), toml::Value::String("/tmp/test.txt".to_string()));
            t
        });
        let result = dest.initialize(config).await;
        assert!(result.is_ok());
        assert!(dest.is_healthy());
    }

    #[tokio::test]
    async fn test_file_dest_initialize_missing_path_fails() {
        let mut dest = FileDestination::new();
        let config = toml::Value::Table(Default::default());
        let result = dest.initialize(config).await;
        match result {
            Err(DestinationError::InitializationFailed(msg)) => {
                assert!(msg.contains("path"));
            }
            _ => panic!("expected InitializationFailed"),
        }
    }

    #[tokio::test]
    async fn test_file_dest_send_text_writes_to_file() {
        let dir = std::env::temp_dir().join("voxmux_file_dest_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("output.txt");
        // Clean up from previous runs
        let _ = std::fs::remove_file(&path);

        let mut dest = FileDestination::new();
        let config = toml::Value::Table({
            let mut t = toml::map::Map::new();
            t.insert(
                "path".to_string(),
                toml::Value::String(path.to_string_lossy().to_string()),
            );
            t
        });
        dest.initialize(config).await.unwrap();

        let metadata = TextMetadata {
            input_id: "mic1".to_string(),
            prefix: "[M1] ".to_string(),
        };
        dest.send_text("hello world", &metadata).await.unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "[M1] hello world\n");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn test_file_dest_send_text_appends() {
        let dir = std::env::temp_dir().join("voxmux_file_dest_append");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("output.txt");
        let _ = std::fs::remove_file(&path);

        let mut dest = FileDestination::new();
        let config = toml::Value::Table({
            let mut t = toml::map::Map::new();
            t.insert(
                "path".to_string(),
                toml::Value::String(path.to_string_lossy().to_string()),
            );
            t
        });
        dest.initialize(config).await.unwrap();

        let metadata = TextMetadata {
            input_id: "mic1".to_string(),
            prefix: "".to_string(),
        };
        dest.send_text("line one", &metadata).await.unwrap();
        dest.send_text("line two", &metadata).await.unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "line one\nline two\n");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn test_file_dest_send_text_before_initialize_fails() {
        let dest = FileDestination::new();
        let metadata = TextMetadata {
            input_id: "mic1".to_string(),
            prefix: "".to_string(),
        };
        let result = dest.send_text("test", &metadata).await;
        match result {
            Err(DestinationError::SendFailed(_)) => {}
            _ => panic!("expected SendFailed"),
        }
    }

    #[test]
    fn test_file_dest_is_healthy_before_init() {
        let dest = FileDestination::new();
        assert!(!dest.is_healthy());
    }

    #[tokio::test]
    async fn test_file_dest_shutdown_succeeds() {
        let dest = FileDestination::new();
        let result = dest.shutdown().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_file_dest_implements_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<FileDestination>();
    }

    #[tokio::test]
    async fn test_file_dest_send_count() {
        let dir = std::env::temp_dir().join("voxmux_file_dest_count");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("output.txt");
        let _ = std::fs::remove_file(&path);

        let mut dest = FileDestination::new();
        let config = toml::Value::Table({
            let mut t = toml::map::Map::new();
            t.insert(
                "path".to_string(),
                toml::Value::String(path.to_string_lossy().to_string()),
            );
            t
        });
        dest.initialize(config).await.unwrap();

        let metadata = TextMetadata {
            input_id: "mic1".to_string(),
            prefix: "".to_string(),
        };
        for _ in 0..3 {
            dest.send_text("msg", &metadata).await.unwrap();
        }
        assert_eq!(dest.send_count(), 3);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
