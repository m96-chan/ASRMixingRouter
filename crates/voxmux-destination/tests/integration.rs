use tokio::sync::mpsc;
use voxmux_core::RecognitionResult;
use voxmux_destination::DestinationHost;

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

#[tokio::test]
async fn test_full_pipeline_single_route() {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut host = DestinationHost::new(rx);

    let dir = std::env::temp_dir().join("voxmux_integ_single");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.txt");
    let _ = std::fs::remove_file(&path);

    host.add_route("mic1", "file", "[M1] ", file_config(&path.to_string_lossy()))
        .await
        .unwrap();
    host.start();

    tx.send(make_result("mic1", "hello world", true)).unwrap();
    drop(tx);

    tokio::time::timeout(std::time::Duration::from_secs(2), host.shutdown())
        .await
        .expect("shutdown timed out");

    let contents = std::fs::read_to_string(&path).unwrap();
    assert_eq!(contents, "[M1] hello world\n");

    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn test_full_pipeline_multiple_inputs() {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut host = DestinationHost::new(rx);

    let dir = std::env::temp_dir().join("voxmux_integ_multi_input");
    std::fs::create_dir_all(&dir).unwrap();
    let path1 = dir.join("mic1.txt");
    let path2 = dir.join("mic2.txt");
    let _ = std::fs::remove_file(&path1);
    let _ = std::fs::remove_file(&path2);

    host.add_route("mic1", "file", "[M1] ", file_config(&path1.to_string_lossy()))
        .await
        .unwrap();
    host.add_route("mic2", "file", "[M2] ", file_config(&path2.to_string_lossy()))
        .await
        .unwrap();
    host.start();

    tx.send(make_result("mic1", "from one", true)).unwrap();
    tx.send(make_result("mic2", "from two", true)).unwrap();
    drop(tx);

    tokio::time::timeout(std::time::Duration::from_secs(2), host.shutdown())
        .await
        .expect("shutdown timed out");

    let c1 = std::fs::read_to_string(&path1).unwrap();
    let c2 = std::fs::read_to_string(&path2).unwrap();
    assert_eq!(c1, "[M1] from one\n");
    assert_eq!(c2, "[M2] from two\n");

    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn test_full_pipeline_fan_out() {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut host = DestinationHost::new(rx);

    let dir = std::env::temp_dir().join("voxmux_integ_fanout");
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
async fn test_full_pipeline_shutdown_after_sender_drop() {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut host = DestinationHost::new(rx);

    let dir = std::env::temp_dir().join("voxmux_integ_drop");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.txt");

    host.add_route("mic1", "file", "", file_config(&path.to_string_lossy()))
        .await
        .unwrap();
    host.start();

    // Drop sender immediately — should not hang
    drop(tx);

    tokio::time::timeout(std::time::Duration::from_secs(2), host.shutdown())
        .await
        .expect("shutdown should not hang after sender drop");

    std::fs::remove_dir_all(&dir).unwrap();
}
