pub mod capture;
pub mod device;
pub mod mixer;
pub mod output;

pub use capture::{CaptureHandle, CaptureNode};
pub use device::DeviceManager;
pub use mixer::{InputHandle, Mixer, MixerHandle};
pub use output::{OutputHandle, OutputNode};

use ringbuf::traits::Split;
use ringbuf::{HeapCons, HeapProd, HeapRb};

/// Create a ring buffer split into producer and consumer halves.
pub fn create_ring_buffer(capacity: usize) -> (HeapProd<f32>, HeapCons<f32>) {
    HeapRb::<f32>::new(capacity).split()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::traits::{Consumer, Producer};

    #[test]
    #[ignore] // Requires audio hardware
    fn test_device_enumeration() {
        let manager = DeviceManager::new();
        let inputs = manager.list_input_devices().unwrap();
        let outputs = manager.list_output_devices().unwrap();
        println!("Input devices: {}", inputs.len());
        for (name, _) in &inputs {
            println!("  - {}", name);
        }
        println!("Output devices: {}", outputs.len());
        for (name, _) in &outputs {
            println!("  - {}", name);
        }
    }

    #[test]
    fn test_ring_buffer_push_pop() {
        let (mut prod, mut cons) = create_ring_buffer(1024);
        let data = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        prod.push_slice(&data);

        let mut output = vec![0.0f32; 5];
        cons.pop_slice(&mut output);
        assert_eq!(output, data);
    }

    #[test]
    fn test_ring_buffer_preserves_sample_order() {
        let (mut prod, mut cons) = create_ring_buffer(1024);
        let data = vec![0.5, -0.5, 0.25, -0.25];
        prod.push_slice(&data);

        let mut output = vec![0.0f32; 4];
        cons.pop_slice(&mut output);
        assert_eq!(output, data);
    }

    #[test]
    fn test_ring_buffer_empty_returns_none() {
        let (_prod, mut cons) = create_ring_buffer(1024);
        assert!(cons.try_pop().is_none());
    }

    #[test]
    fn test_ring_buffer_overflow_behavior() {
        let (mut prod, _cons) = create_ring_buffer(4);
        // Fill the buffer
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let pushed = prod.push_slice(&data);
        assert_eq!(pushed, 4);
        // Buffer is full — additional push should be rejected
        let overflow_data = vec![5.0, 6.0];
        let pushed = prod.push_slice(&overflow_data);
        assert_eq!(pushed, 0);
    }
}
