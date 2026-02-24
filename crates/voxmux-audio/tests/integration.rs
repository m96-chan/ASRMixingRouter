use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::HeapRb;

#[test]
fn test_passthrough_pipeline() {
    let (mut prod, mut cons) = voxmux_audio::create_ring_buffer(4096);

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

#[test]
fn test_multi_input_mix_to_output_buffer() {
    let (out_prod, mut out_cons) = voxmux_audio::create_ring_buffer(8192);
    let mut mixer = voxmux_audio::Mixer::new(out_prod, 512);

    // 3 inputs with different volumes/mute states
    let (mut prod_a, cons_a) = HeapRb::<f32>::new(4096).split();
    let (mut prod_b, cons_b) = HeapRb::<f32>::new(4096).split();
    let (mut prod_c, cons_c) = HeapRb::<f32>::new(4096).split();

    let handle_a = mixer.add_input("radio1", cons_a, 1.0, false);
    let handle_b = mixer.add_input("radio2", cons_b, 0.5, false);
    let handle_c = mixer.add_input("radio3", cons_c, 1.0, true); // starts muted

    // Feed identical 1.0 signals
    let signal = vec![1.0f32; 256];
    prod_a.push_slice(&signal);
    prod_b.push_slice(&signal);
    prod_c.push_slice(&signal);

    // Mix cycle 1: a(1.0*1.0) + b(1.0*0.5) + c(muted) = 1.5
    let written = mixer.mix_once();
    assert_eq!(written, 256);
    let mut result = vec![0.0f32; 256];
    out_cons.pop_slice(&mut result);
    for s in &result {
        assert!((s - 1.5).abs() < 1e-6, "expected 1.5, got {}", s);
    }

    // Change: mute a, unmute c, change b gain to 0.25
    handle_a.set_muted(true);
    handle_b.set_volume(0.25);
    handle_c.set_muted(false);

    prod_a.push_slice(&signal);
    prod_b.push_slice(&signal);
    prod_c.push_slice(&signal);

    // Mix cycle 2: a(muted) + b(1.0*0.25) + c(1.0*1.0) = 1.25
    let written = mixer.mix_once();
    assert_eq!(written, 256);
    let mut result2 = vec![0.0f32; 256];
    out_cons.pop_slice(&mut result2);
    for s in &result2 {
        assert!((s - 1.25).abs() < 1e-6, "expected 1.25, got {}", s);
    }

    drop(handle_a);
    drop(handle_b);
    drop(handle_c);
}

#[test]
fn test_mixer_with_threaded_producers() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    let (out_prod, mut out_cons) = voxmux_audio::create_ring_buffer(16384);
    let mut mixer = voxmux_audio::Mixer::new(out_prod, 512);

    // Create 3 producer→consumer pairs
    let mut producer_handles = Vec::new();
    let done = Arc::new(AtomicBool::new(false));

    for i in 0..3 {
        let (prod, cons) = HeapRb::<f32>::new(4096).split();
        let _h = mixer.add_input(&format!("input_{}", i), cons, 1.0, false);

        let done_flag = Arc::clone(&done);
        let handle = std::thread::spawn(move || {
            let mut prod = prod;
            let val = (i + 1) as f32 * 0.1; // 0.1, 0.2, 0.3
            while !done_flag.load(Ordering::Relaxed) {
                prod.push_slice(&vec![val; 64]);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        producer_handles.push(handle);
    }

    // Start mixer thread
    let mixer_handle = mixer.start(Duration::from_millis(1));

    // Let it run for a bit
    std::thread::sleep(Duration::from_millis(100));

    // Stop everything
    done.store(true, Ordering::Relaxed);
    mixer_handle.stop();
    for h in producer_handles {
        h.join().unwrap();
    }

    // Verify some data came through (sum should be ~0.6 per sample)
    let mut buf = vec![0.0f32; 16384];
    let n = out_cons.pop_slice(&mut buf);
    assert!(n > 0, "expected output data from threaded mixer");

    // Samples should be approximately 0.1 + 0.2 + 0.3 = 0.6
    // (Allow some tolerance for partial buffer fills)
    let non_zero: Vec<&f32> = buf[..n].iter().filter(|&&s| s > 0.01).collect();
    assert!(!non_zero.is_empty(), "expected non-zero samples in output");
}
