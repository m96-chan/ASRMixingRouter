use crate::dest_trait::Destination;
use crate::registry::DestinationRegistry;
use std::collections::HashMap;
use tokio::sync::mpsc;
use voxmux_core::{DestinationError, RecognitionResult, TextMetadata};

struct Route {
    destination: Box<dyn Destination>,
    prefix: String,
}

pub struct DestinationHost {
    registry: DestinationRegistry,
    routes: HashMap<String, Vec<Route>>,
    result_rx: Option<mpsc::UnboundedReceiver<RecognitionResult>>,
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl DestinationHost {
    pub fn new(result_rx: mpsc::UnboundedReceiver<RecognitionResult>) -> Self {
        Self {
            registry: DestinationRegistry::new(),
            routes: HashMap::new(),
            result_rx: Some(result_rx),
            task_handle: None,
        }
    }

    pub async fn add_route(
        &mut self,
        input_id: &str,
        plugin_name: &str,
        prefix: &str,
        config: toml::Value,
    ) -> Result<(), DestinationError> {
        let mut dest = self.registry.create(plugin_name)?;
        dest.initialize(config).await?;

        let route = Route {
            destination: dest,
            prefix: prefix.to_string(),
        };

        self.routes
            .entry(input_id.to_string())
            .or_default()
            .push(route);

        Ok(())
    }

    pub fn start(&mut self) {
        let mut rx = self
            .result_rx
            .take()
            .expect("start() called but receiver already taken");
        let routes = std::mem::take(&mut self.routes);

        let handle = tokio::spawn(async move {
            while let Some(result) = rx.recv().await {
                if !result.is_final {
                    continue;
                }

                if let Some(input_routes) = routes.get(&result.input_id) {
                    for route in input_routes {
                        let metadata = TextMetadata {
                            input_id: result.input_id.clone(),
                            prefix: route.prefix.clone(),
                        };
                        if let Err(e) = route.destination.send_text(&result.text, &metadata).await
                        {
                            tracing::error!(
                                input_id = %result.input_id,
                                destination = %route.destination.name(),
                                "send_text failed: {e}"
                            );
                        }
                    }
                }
            }
        });

        self.task_handle = Some(handle);
    }

    pub async fn shutdown(&mut self) {
        if let Some(handle) = self.task_handle.take() {
            let _ = handle.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_channel() -> (
        mpsc::UnboundedSender<RecognitionResult>,
        mpsc::UnboundedReceiver<RecognitionResult>,
    ) {
        mpsc::unbounded_channel()
    }

    fn make_result(input_id: &str, text: &str, is_final: bool) -> RecognitionResult {
        RecognitionResult {
            text: text.to_string(),
            input_id: input_id.to_string(),
            timestamp: 0.0,
            is_final,
        }
    }

    fn file_config(path: &str) -> toml::Value {
        toml::Value::Table({
            let mut t = toml::map::Map::new();
            t.insert("path".to_string(), toml::Value::String(path.to_string()));
            t
        })
    }

    #[test]
    fn test_host_new_creates_successfully() {
        let (_tx, rx) = make_channel();
        let _host = DestinationHost::new(rx);
    }

    #[tokio::test]
    async fn test_host_add_route_returns_ok() {
        let (_tx, rx) = make_channel();
        let mut host = DestinationHost::new(rx);
        let dir = std::env::temp_dir().join("voxmux_host_add_route");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.txt");

        let result = host
            .add_route("mic1", "file", "[M1] ", file_config(&path.to_string_lossy()))
            .await;
        assert!(result.is_ok());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn test_host_add_route_unknown_plugin_fails() {
        let (_tx, rx) = make_channel();
        let mut host = DestinationHost::new(rx);
        let result = host
            .add_route("mic1", "nonexistent", "", toml::Value::Table(Default::default()))
            .await;
        match result {
            Err(DestinationError::NotFound(_)) => {}
            _ => panic!("expected NotFound"),
        }
    }

    #[tokio::test]
    async fn test_host_start_and_send_result_routes_to_file() {
        let (tx, rx) = make_channel();
        let mut host = DestinationHost::new(rx);
        let dir = std::env::temp_dir().join("voxmux_host_route_file");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.txt");
        let _ = std::fs::remove_file(&path);

        host.add_route("mic1", "file", "[M1] ", file_config(&path.to_string_lossy()))
            .await
            .unwrap();
        host.start();

        tx.send(make_result("mic1", "hello", true)).unwrap();
        drop(tx);

        tokio::time::timeout(std::time::Duration::from_secs(2), host.shutdown())
            .await
            .expect("shutdown timed out");

        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "[M1] hello\n");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn test_host_routes_to_correct_input() {
        let (tx, rx) = make_channel();
        let mut host = DestinationHost::new(rx);
        let dir = std::env::temp_dir().join("voxmux_host_correct_input");
        std::fs::create_dir_all(&dir).unwrap();
        let path1 = dir.join("mic1.txt");
        let path2 = dir.join("mic2.txt");
        let _ = std::fs::remove_file(&path1);
        let _ = std::fs::remove_file(&path2);

        host.add_route("mic1", "file", "", file_config(&path1.to_string_lossy()))
            .await
            .unwrap();
        host.add_route("mic2", "file", "", file_config(&path2.to_string_lossy()))
            .await
            .unwrap();
        host.start();

        tx.send(make_result("mic1", "from mic1", true)).unwrap();
        drop(tx);

        tokio::time::timeout(std::time::Duration::from_secs(2), host.shutdown())
            .await
            .expect("shutdown timed out");

        let contents1 = std::fs::read_to_string(&path1).unwrap();
        assert_eq!(contents1, "from mic1\n");
        // mic2 file should not exist since nothing was routed to it
        assert!(!path2.exists());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn test_host_multiple_destinations_per_input() {
        let (tx, rx) = make_channel();
        let mut host = DestinationHost::new(rx);
        let dir = std::env::temp_dir().join("voxmux_host_fanout");
        std::fs::create_dir_all(&dir).unwrap();
        let path_a = dir.join("a.txt");
        let path_b = dir.join("b.txt");
        let _ = std::fs::remove_file(&path_a);
        let _ = std::fs::remove_file(&path_b);

        host.add_route("mic1", "file", "[A] ", file_config(&path_a.to_string_lossy()))
            .await
            .unwrap();
        host.add_route("mic1", "file", "[B] ", file_config(&path_b.to_string_lossy()))
            .await
            .unwrap();
        host.start();

        tx.send(make_result("mic1", "fanout", true)).unwrap();
        drop(tx);

        tokio::time::timeout(std::time::Duration::from_secs(2), host.shutdown())
            .await
            .expect("shutdown timed out");

        let a = std::fs::read_to_string(&path_a).unwrap();
        let b = std::fs::read_to_string(&path_b).unwrap();
        assert_eq!(a, "[A] fanout\n");
        assert_eq!(b, "[B] fanout\n");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn test_host_ignores_unrouted_input() {
        let (tx, rx) = make_channel();
        let mut host = DestinationHost::new(rx);
        let dir = std::env::temp_dir().join("voxmux_host_unrouted");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.txt");
        let _ = std::fs::remove_file(&path);

        host.add_route("mic1", "file", "", file_config(&path.to_string_lossy()))
            .await
            .unwrap();
        host.start();

        // Send result for an unrouted input — should not crash
        tx.send(make_result("mic_unknown", "ignored", true)).unwrap();
        drop(tx);

        tokio::time::timeout(std::time::Duration::from_secs(2), host.shutdown())
            .await
            .expect("shutdown timed out");

        // File should be empty (or not exist) since nothing matched mic1
        let exists = path.exists();
        if exists {
            let contents = std::fs::read_to_string(&path).unwrap();
            assert!(contents.is_empty());
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn test_host_shutdown_completes() {
        let (_tx, rx) = make_channel();
        let mut host = DestinationHost::new(rx);
        host.start();

        // Drop the sender so the task finishes
        drop(_tx);

        tokio::time::timeout(std::time::Duration::from_secs(2), host.shutdown())
            .await
            .expect("shutdown timed out");
    }

    #[tokio::test]
    async fn test_host_processes_multiple_results() {
        let (tx, rx) = make_channel();
        let mut host = DestinationHost::new(rx);
        let dir = std::env::temp_dir().join("voxmux_host_multi_results");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.txt");
        let _ = std::fs::remove_file(&path);

        host.add_route("mic1", "file", "", file_config(&path.to_string_lossy()))
            .await
            .unwrap();
        host.start();

        tx.send(make_result("mic1", "one", true)).unwrap();
        tx.send(make_result("mic1", "two", true)).unwrap();
        tx.send(make_result("mic1", "three", true)).unwrap();
        drop(tx);

        tokio::time::timeout(std::time::Duration::from_secs(2), host.shutdown())
            .await
            .expect("shutdown timed out");

        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "one\ntwo\nthree\n");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn test_host_only_routes_final_results() {
        let (tx, rx) = make_channel();
        let mut host = DestinationHost::new(rx);
        let dir = std::env::temp_dir().join("voxmux_host_final_only");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.txt");
        let _ = std::fs::remove_file(&path);

        host.add_route("mic1", "file", "", file_config(&path.to_string_lossy()))
            .await
            .unwrap();
        host.start();

        tx.send(make_result("mic1", "partial", false)).unwrap();
        tx.send(make_result("mic1", "final", true)).unwrap();
        drop(tx);

        tokio::time::timeout(std::time::Duration::from_secs(2), host.shutdown())
            .await
            .expect("shutdown timed out");

        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "final\n");

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
