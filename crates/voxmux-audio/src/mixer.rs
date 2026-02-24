use ringbuf::traits::{Consumer, Producer};
use ringbuf::{HeapCons, HeapProd};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

// ── InputControls ──────────────────────────────────────────────

pub struct InputControls {
    volume_bits: AtomicU32,
    muted: AtomicBool,
    id: String,
}

impl InputControls {
    pub fn new(id: &str, volume: f32, muted: bool) -> Self {
        Self {
            volume_bits: AtomicU32::new(volume.to_bits()),
            muted: AtomicBool::new(muted),
            id: id.to_string(),
        }
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }

    pub fn set_volume(&self, v: f32) {
        self.volume_bits.store(v.to_bits(), Ordering::Relaxed);
    }

    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    pub fn set_muted(&self, m: bool) {
        self.muted.store(m, Ordering::Relaxed);
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

// ── InputHandle ────────────────────────────────────────────────

#[derive(Clone)]
pub struct InputHandle {
    controls: Arc<InputControls>,
}

impl InputHandle {
    fn from_arc(controls: Arc<InputControls>) -> Self {
        Self { controls }
    }

    pub fn volume(&self) -> f32 {
        self.controls.volume()
    }

    pub fn set_volume(&self, v: f32) {
        self.controls.set_volume(v.max(0.0));
    }

    pub fn is_muted(&self) -> bool {
        self.controls.is_muted()
    }

    pub fn set_muted(&self, m: bool) {
        self.controls.set_muted(m);
    }

    pub fn id(&self) -> &str {
        self.controls.id()
    }
}

// ── MixerInput ─────────────────────────────────────────────────

struct MixerInput {
    consumer: HeapCons<f32>,
    controls: Arc<InputControls>,
}

// ── Mixer ──────────────────────────────────────────────────────

pub struct Mixer {
    inputs: Vec<MixerInput>,
    output: HeapProd<f32>,
    mix_buffer: Vec<f32>,
    read_buffer: Vec<f32>,
}

impl Mixer {
    pub fn new(output: HeapProd<f32>, mix_block_size: usize) -> Self {
        Self {
            inputs: Vec::new(),
            output,
            mix_buffer: vec![0.0; mix_block_size],
            read_buffer: vec![0.0; mix_block_size],
        }
    }

    pub fn add_input(
        &mut self,
        id: &str,
        consumer: HeapCons<f32>,
        volume: f32,
        muted: bool,
    ) -> InputHandle {
        let controls = Arc::new(InputControls::new(id, volume, muted));
        let handle = InputHandle::from_arc(Arc::clone(&controls));
        self.inputs.push(MixerInput { consumer, controls });
        handle
    }

    /// Run one mix cycle: drain all inputs, apply gain, sum, write to output.
    /// Returns the number of samples pushed to the output.
    pub fn mix_once(&mut self) -> usize {
        if self.inputs.is_empty() {
            return 0;
        }

        let block = self.mix_buffer.len();

        // Zero mix buffer
        self.mix_buffer.iter_mut().for_each(|s| *s = 0.0);

        let mut max_read = 0usize;

        for input in &mut self.inputs {
            // Always drain to prevent stale data buildup
            self.read_buffer.iter_mut().for_each(|s| *s = 0.0);
            let n = input.consumer.pop_slice(&mut self.read_buffer[..block]);
            if n > max_read {
                max_read = n;
            }

            if !input.controls.is_muted() {
                let vol = input.controls.volume();
                for i in 0..n {
                    self.mix_buffer[i] += self.read_buffer[i] * vol;
                }
            }
        }

        if max_read == 0 {
            return 0;
        }

        // Push mixed samples to output
        self.output.push_slice(&self.mix_buffer[..max_read])
    }

    /// Run the mixer loop until `running` is set to false.
    pub fn run(&mut self, running: Arc<AtomicBool>, interval: std::time::Duration) {
        while running.load(Ordering::Relaxed) {
            self.mix_once();
            std::thread::sleep(interval);
        }
    }

    /// Spawn the mixer on a dedicated thread. Consumes self.
    /// Returns a `MixerHandle` that can stop the thread.
    pub fn start(mut self, interval: std::time::Duration) -> MixerHandle {
        let running = Arc::new(AtomicBool::new(true));
        let flag = Arc::clone(&running);
        let thread = std::thread::Builder::new()
            .name("mixer".into())
            .spawn(move || {
                self.run(flag, interval);
            })
            .expect("failed to spawn mixer thread");
        MixerHandle {
            running,
            thread: Some(thread),
            input_handles: Vec::new(),
        }
    }
}

// ── MixerHandle ────────────────────────────────────────────────

pub struct MixerHandle {
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    input_handles: Vec<InputHandle>,
}

impl MixerHandle {
    /// Signal the mixer thread to stop and wait for it to finish.
    pub fn stop(mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            t.join().expect("mixer thread panicked");
        }
    }

    pub fn input_handles(&self) -> &[InputHandle] {
        &self.input_handles
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::traits::Split;
    use ringbuf::HeapRb;

    impl InputHandle {
        fn new(id: &str, volume: f32, muted: bool) -> Self {
            Self {
                controls: Arc::new(InputControls::new(id, volume, muted)),
            }
        }
    }

    // ── Group A: InputControls & InputHandle ───────────────────

    #[test]
    fn test_input_controls_default_volume() {
        let ctrl = InputControls::new("test", 1.0, false);
        assert_eq!(ctrl.volume(), 1.0);
        assert!(!ctrl.is_muted());
    }

    #[test]
    fn test_input_controls_volume_roundtrip() {
        let ctrl = InputControls::new("test", 0.0, false);
        for &v in &[0.0_f32, 0.5, 1.0, 0.001, 2.5] {
            ctrl.set_volume(v);
            assert_eq!(ctrl.volume(), v);
        }
    }

    #[test]
    fn test_input_controls_muted_roundtrip() {
        let ctrl = InputControls::new("test", 1.0, false);
        assert!(!ctrl.is_muted());
        ctrl.set_muted(true);
        assert!(ctrl.is_muted());
        ctrl.set_muted(false);
        assert!(!ctrl.is_muted());
    }

    #[test]
    fn test_input_handle_set_get_volume() {
        let handle = InputHandle::new("h", 0.75, false);
        assert_eq!(handle.volume(), 0.75);
        handle.set_volume(0.3);
        assert_eq!(handle.volume(), 0.3);
    }

    #[test]
    fn test_input_handle_set_get_muted() {
        let handle = InputHandle::new("h", 1.0, false);
        assert!(!handle.is_muted());
        handle.set_muted(true);
        assert!(handle.is_muted());
    }

    #[test]
    fn test_input_handle_volume_clamps_negative() {
        let handle = InputHandle::new("h", 1.0, false);
        handle.set_volume(-1.0);
        assert_eq!(handle.volume(), 0.0);
    }

    #[test]
    fn test_input_handle_id() {
        let handle = InputHandle::new("radio_1", 1.0, false);
        assert_eq!(handle.id(), "radio_1");
    }

    #[test]
    fn test_input_handle_clone_shares_state() {
        let h1 = InputHandle::new("shared", 1.0, false);
        let h2 = h1.clone();
        h1.set_volume(0.42);
        assert_eq!(h2.volume(), 0.42);
        h2.set_muted(true);
        assert!(h1.is_muted());
    }

    // ── Group B: Mixer core mix_once ────────────────────────────

    /// Helper: create a Mixer with an output ring buffer, returning (mixer, output_consumer).
    fn make_mixer(block_size: usize, out_capacity: usize) -> (Mixer, HeapCons<f32>) {
        let (prod, cons) = HeapRb::<f32>::new(out_capacity).split();
        (Mixer::new(prod, block_size), cons)
    }

    /// Helper: push samples into a producer and return the consumer for mixer input.
    fn feed(samples: &[f32], capacity: usize) -> HeapCons<f32> {
        let (mut prod, cons) = HeapRb::<f32>::new(capacity).split();
        prod.push_slice(samples);
        cons
    }

    #[test]
    fn test_mixer_no_inputs_writes_nothing() {
        let (mut mixer, mut out) = make_mixer(128, 1024);
        let written = mixer.mix_once();
        assert_eq!(written, 0);
        assert!(out.try_pop().is_none());
    }

    #[test]
    fn test_mixer_single_input_passthrough() {
        let (mut mixer, mut out) = make_mixer(128, 1024);
        let samples: Vec<f32> = (0..64).map(|i| i as f32 * 0.01).collect();
        let cons = feed(&samples, 256);
        let _h = mixer.add_input("a", cons, 1.0, false);

        let written = mixer.mix_once();
        assert_eq!(written, 64);

        let mut result = vec![0.0f32; 64];
        out.pop_slice(&mut result);
        assert_eq!(result, samples);
    }

    #[test]
    fn test_mixer_single_input_with_gain() {
        let (mut mixer, mut out) = make_mixer(128, 1024);
        let samples = vec![1.0f32; 32];
        let cons = feed(&samples, 256);
        let _h = mixer.add_input("a", cons, 0.5, false);

        mixer.mix_once();

        let mut result = vec![0.0f32; 32];
        out.pop_slice(&mut result);
        for s in &result {
            assert!((s - 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn test_mixer_single_input_muted() {
        let (mut mixer, mut out) = make_mixer(128, 1024);
        let samples = vec![1.0f32; 32];
        let cons = feed(&samples, 256);
        let _h = mixer.add_input("a", cons, 1.0, true);

        let written = mixer.mix_once();
        // Muted → silence written (zeros) since data was drained
        assert_eq!(written, 32);

        let mut result = vec![0.0f32; 32];
        out.pop_slice(&mut result);
        for s in &result {
            assert_eq!(*s, 0.0);
        }
    }

    #[test]
    fn test_mixer_two_inputs_summed() {
        let (mut mixer, mut out) = make_mixer(128, 1024);
        let a = vec![0.3f32; 16];
        let b = vec![0.4f32; 16];
        let _ha = mixer.add_input("a", feed(&a, 256), 1.0, false);
        let _hb = mixer.add_input("b", feed(&b, 256), 1.0, false);

        mixer.mix_once();

        let mut result = vec![0.0f32; 16];
        out.pop_slice(&mut result);
        for s in &result {
            assert!((s - 0.7).abs() < 1e-6);
        }
    }

    #[test]
    fn test_mixer_two_inputs_different_gains() {
        let (mut mixer, mut out) = make_mixer(128, 1024);
        let a = vec![1.0f32; 16];
        let b = vec![1.0f32; 16];
        let _ha = mixer.add_input("a", feed(&a, 256), 0.5, false);
        let _hb = mixer.add_input("b", feed(&b, 256), 0.25, false);

        mixer.mix_once();

        let mut result = vec![0.0f32; 16];
        out.pop_slice(&mut result);
        for s in &result {
            assert!((s - 0.75).abs() < 1e-6);
        }
    }

    #[test]
    fn test_mixer_one_muted_one_active() {
        let (mut mixer, mut out) = make_mixer(128, 1024);
        let a = vec![0.5f32; 16];
        let b = vec![0.9f32; 16];
        let _ha = mixer.add_input("a", feed(&a, 256), 1.0, true);
        let _hb = mixer.add_input("b", feed(&b, 256), 1.0, false);

        mixer.mix_once();

        let mut result = vec![0.0f32; 16];
        out.pop_slice(&mut result);
        for s in &result {
            assert!((s - 0.9).abs() < 1e-6);
        }
    }

    #[test]
    fn test_mixer_empty_input_contributes_zero() {
        let (mut mixer, mut out) = make_mixer(128, 1024);
        let a: Vec<f32> = vec![]; // empty
        let b = vec![0.6f32; 16];
        let _ha = mixer.add_input("a", feed(&a, 256), 1.0, false);
        let _hb = mixer.add_input("b", feed(&b, 256), 1.0, false);

        let written = mixer.mix_once();
        assert_eq!(written, 16);

        let mut result = vec![0.0f32; 16];
        out.pop_slice(&mut result);
        for s in &result {
            assert!((s - 0.6).abs() < 1e-6);
        }
    }

    #[test]
    fn test_mixer_partial_input_buffer() {
        let (mut mixer, mut out) = make_mixer(128, 1024);
        // "a" has 64 samples, "b" has 128 samples
        let a: Vec<f32> = vec![0.2; 64];
        let b: Vec<f32> = vec![0.3; 128];
        let _ha = mixer.add_input("a", feed(&a, 256), 1.0, false);
        let _hb = mixer.add_input("b", feed(&b, 256), 1.0, false);

        let written = mixer.mix_once();
        assert_eq!(written, 128);

        let mut result = vec![0.0f32; 128];
        out.pop_slice(&mut result);
        // First 64: 0.2 + 0.3 = 0.5
        for s in &result[..64] {
            assert!((s - 0.5).abs() < 1e-6, "expected 0.5, got {}", s);
        }
        // Last 64: only "b" → 0.3
        for s in &result[64..] {
            assert!((s - 0.3).abs() < 1e-6, "expected 0.3, got {}", s);
        }
    }

    #[test]
    fn test_mixer_runtime_volume_change() {
        let (mut mixer, mut out) = make_mixer(128, 4096);
        // Use a ring buffer big enough for 2 mix cycles
        let (mut prod, cons) = HeapRb::<f32>::new(512).split();
        prod.push_slice(&vec![1.0f32; 128]);
        let handle = mixer.add_input("a", cons, 1.0, false);

        // First cycle: volume = 1.0
        mixer.mix_once();
        let mut r1 = vec![0.0f32; 128];
        out.pop_slice(&mut r1);
        assert!((r1[0] - 1.0).abs() < 1e-6);

        // Change volume, push more data
        handle.set_volume(0.25);
        prod.push_slice(&vec![1.0f32; 128]);

        mixer.mix_once();
        let mut r2 = vec![0.0f32; 128];
        out.pop_slice(&mut r2);
        assert!((r2[0] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn test_mixer_runtime_mute_toggle() {
        let (mut mixer, mut out) = make_mixer(128, 4096);
        let (mut prod, cons) = HeapRb::<f32>::new(512).split();
        prod.push_slice(&vec![0.8f32; 64]);
        let handle = mixer.add_input("a", cons, 1.0, false);

        // Unmuted
        mixer.mix_once();
        let mut r1 = vec![0.0f32; 64];
        out.pop_slice(&mut r1);
        assert!((r1[0] - 0.8).abs() < 1e-6);

        // Mute
        handle.set_muted(true);
        prod.push_slice(&vec![0.8f32; 64]);
        mixer.mix_once();
        let mut r2 = vec![0.0f32; 64];
        out.pop_slice(&mut r2);
        assert_eq!(r2[0], 0.0);

        // Unmute
        handle.set_muted(false);
        prod.push_slice(&vec![0.8f32; 64]);
        mixer.mix_once();
        let mut r3 = vec![0.0f32; 64];
        out.pop_slice(&mut r3);
        assert!((r3[0] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_mixer_three_inputs() {
        let (mut mixer, mut out) = make_mixer(128, 1024);
        let a = vec![1.0f32; 32];
        let b = vec![1.0f32; 32];
        let c = vec![1.0f32; 32];
        let _ha = mixer.add_input("a", feed(&a, 256), 0.2, false);
        let _hb = mixer.add_input("b", feed(&b, 256), 0.3, false);
        let _hc = mixer.add_input("c", feed(&c, 256), 0.5, false);

        mixer.mix_once();

        let mut result = vec![0.0f32; 32];
        out.pop_slice(&mut result);
        for s in &result {
            assert!((s - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_mixer_output_full_limits_write() {
        // Output buffer has only 4 slots
        let (mut mixer, _out) = make_mixer(128, 4);
        let samples = vec![1.0f32; 64];
        let _h = mixer.add_input("a", feed(&samples, 256), 1.0, false);

        let written = mixer.mix_once();
        assert_eq!(written, 4);
    }

    #[test]
    fn test_mixer_all_inputs_empty_writes_nothing() {
        let (mut mixer, mut out) = make_mixer(128, 1024);
        let empty: Vec<f32> = vec![];
        let _ha = mixer.add_input("a", feed(&empty, 256), 1.0, false);
        let _hb = mixer.add_input("b", feed(&empty, 256), 1.0, false);

        let written = mixer.mix_once();
        assert_eq!(written, 0);
        assert!(out.try_pop().is_none());
    }

    #[test]
    fn test_mixer_preserves_signal_shape() {
        let (mut mixer, mut out) = make_mixer(256, 4096);
        let sine: Vec<f32> = (0..200)
            .map(|i| (i as f32 * 0.05 * std::f32::consts::TAU).sin())
            .collect();
        let cons = feed(&sine, 512);
        let _h = mixer.add_input("sine", cons, 1.0, false);

        mixer.mix_once();

        let mut result = vec![0.0f32; 200];
        out.pop_slice(&mut result);
        for (a, b) in result.iter().zip(sine.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    // ── Group C: Mixer thread lifecycle ─────────────────────────

    #[test]
    fn test_mixer_run_stops_on_flag() {
        let (mut mixer, _out) = make_mixer(128, 1024);
        let _h = mixer.add_input("a", feed(&[], 256), 1.0, false);

        let running = Arc::new(AtomicBool::new(true));
        let flag = Arc::clone(&running);

        // Stop flag from another thread after a short delay
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            flag.store(false, Ordering::Relaxed);
        });

        mixer.run(running, std::time::Duration::from_millis(5));
        stopper.join().unwrap();
        // If we reach here, run() exited — test passes
    }

    #[test]
    fn test_mixer_start_and_stop() {
        let (mixer, _out) = make_mixer(128, 1024);
        let handle = mixer.start(std::time::Duration::from_millis(5));
        std::thread::sleep(std::time::Duration::from_millis(30));
        handle.stop();
        // If stop() returns without hanging, test passes
    }

    #[test]
    fn test_mixer_thread_processes_data() {
        let (out_prod, mut out_cons) = HeapRb::<f32>::new(4096).split();
        let mut mixer = Mixer::new(out_prod, 256);

        let (mut in_prod, in_cons) = HeapRb::<f32>::new(4096).split();
        let _h = mixer.add_input("a", in_cons, 1.0, false);

        let handle = mixer.start(std::time::Duration::from_millis(1));

        // Feed data while mixer thread is running
        in_prod.push_slice(&vec![0.5f32; 256]);

        // Give the mixer thread time to process
        std::thread::sleep(std::time::Duration::from_millis(50));

        handle.stop();

        // Verify data came through
        let mut result = vec![0.0f32; 256];
        let n = out_cons.pop_slice(&mut result);
        assert!(n > 0, "mixer thread should have processed some samples");
        for s in &result[..n] {
            assert!((s - 0.5).abs() < 1e-6);
        }
    }
}
