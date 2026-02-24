use ringbuf::traits::{Consumer, Producer};

#[test]
fn test_passthrough_pipeline() {
    let (mut prod, mut cons) = asr_audio::create_ring_buffer(4096);

    // Simulate a sine-wave-like signal
    let signal: Vec<f32> = (0..1000)
        .map(|i| (i as f32 * 0.01).sin())
        .collect();

    let pushed = prod.push_slice(&signal);
    assert_eq!(pushed, signal.len());

    let mut output = vec![0.0f32; signal.len()];
    cons.pop_slice(&mut output);

    // Verify the output is identical to the input
    assert_eq!(output, signal);
}
