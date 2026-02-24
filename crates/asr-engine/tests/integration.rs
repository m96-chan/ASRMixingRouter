use asr_core::AudioChunk;
use asr_engine::{AsrHost, PluginRegistry};

#[tokio::test]
async fn test_full_pipeline_null_engine() {
    let registry = PluginRegistry::new();
    let mut host = AsrHost::new();
    let mut rx = host.take_result_receiver().unwrap();

    let tx = host
        .add_input("mic1", "null", toml::Value::Table(Default::default()), &registry)
        .await
        .unwrap();
    host.start();

    let chunk = AudioChunk {
        samples: vec![0.0; 480],
        sample_rate: 48000,
        channels: 1,
    };
    tx.send(chunk).unwrap();

    let result = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert_eq!(result.input_id, "mic1");
    assert!(result.text.contains("480"));
    assert!(result.is_final);

    drop(tx);
    host.shutdown().await;
}

#[tokio::test]
async fn test_full_pipeline_two_inputs() {
    let registry = PluginRegistry::new();
    let mut host = AsrHost::new();
    let mut rx = host.take_result_receiver().unwrap();

    let tx1 = host
        .add_input("radio1", "null", toml::Value::Table(Default::default()), &registry)
        .await
        .unwrap();
    let tx2 = host
        .add_input("radio2", "null", toml::Value::Table(Default::default()), &registry)
        .await
        .unwrap();
    host.start();

    tx1.send(AudioChunk {
        samples: vec![0.0; 100],
        sample_rate: 48000,
        channels: 1,
    })
    .unwrap();
    tx2.send(AudioChunk {
        samples: vec![0.0; 200],
        sample_rate: 48000,
        channels: 1,
    })
    .unwrap();

    let timeout = std::time::Duration::from_secs(2);
    let r1 = tokio::time::timeout(timeout, rx.recv())
        .await
        .expect("timed out")
        .expect("closed");
    let r2 = tokio::time::timeout(timeout, rx.recv())
        .await
        .expect("timed out")
        .expect("closed");

    let mut ids = vec![r1.input_id.clone(), r2.input_id.clone()];
    ids.sort();
    assert_eq!(ids, vec!["radio1", "radio2"]);

    drop(tx1);
    drop(tx2);
    host.shutdown().await;
}

#[tokio::test]
async fn test_full_pipeline_shutdown_after_drop() {
    let registry = PluginRegistry::new();
    let mut host = AsrHost::new();

    let tx = host
        .add_input("mic1", "null", toml::Value::Table(Default::default()), &registry)
        .await
        .unwrap();
    host.start();

    drop(tx);

    // Shutdown should complete without hanging
    tokio::time::timeout(std::time::Duration::from_secs(2), host.shutdown())
        .await
        .expect("shutdown timed out");
}

#[tokio::test]
async fn test_full_pipeline_large_chunk() {
    let registry = PluginRegistry::new();
    let mut host = AsrHost::new();
    let mut rx = host.take_result_receiver().unwrap();

    let tx = host
        .add_input("mic1", "null", toml::Value::Table(Default::default()), &registry)
        .await
        .unwrap();
    host.start();

    // 10 seconds of audio at 48kHz
    let chunk = AudioChunk {
        samples: vec![0.0; 480_000],
        sample_rate: 48000,
        channels: 1,
    };
    tx.send(chunk).unwrap();

    let result = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert!(result.text.contains("480000"));

    drop(tx);
    host.shutdown().await;
}
