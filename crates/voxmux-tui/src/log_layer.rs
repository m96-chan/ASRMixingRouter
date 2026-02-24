use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::Subscriber;
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// A tracing layer that captures formatted log events into a bounded buffer.
pub struct TuiLogLayer {
    buffer: Arc<Mutex<VecDeque<String>>>,
    capacity: usize,
}

impl TuiLogLayer {
    pub fn new(buffer: Arc<Mutex<VecDeque<String>>>, capacity: usize) -> Self {
        Self { buffer, capacity }
    }
}

struct MessageVisitor {
    message: String,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }
}

impl<S: Subscriber> Layer<S> for TuiLogLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let level = metadata.level();
        let target = metadata.target();

        let mut visitor = MessageVisitor {
            message: String::new(),
        };
        event.record(&mut visitor);

        let formatted = format!("[{}] {}: {}", level, target, visitor.message);

        if let Ok(mut buf) = self.buffer.lock() {
            if buf.len() >= self.capacity {
                buf.pop_front();
            }
            buf.push_back(formatted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::Registry;

    fn make_layer_and_buffer(
        capacity: usize,
    ) -> (Arc<Mutex<VecDeque<String>>>, impl tracing::Subscriber) {
        let buffer = Arc::new(Mutex::new(VecDeque::new()));
        let layer = TuiLogLayer::new(Arc::clone(&buffer), capacity);
        let subscriber = Registry::default().with(layer);
        (buffer, subscriber)
    }

    #[test]
    fn test_log_layer_captures_events() {
        let (buffer, subscriber) = make_layer_and_buffer(100);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("one");
            tracing::warn!("two");
            tracing::error!("three");
        });
        let buf = buffer.lock().unwrap();
        assert_eq!(buf.len(), 3);
    }

    #[test]
    fn test_log_layer_bounded_drops_oldest() {
        let (buffer, subscriber) = make_layer_and_buffer(2);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("first");
            tracing::info!("second");
            tracing::info!("third");
        });
        let buf = buffer.lock().unwrap();
        assert_eq!(buf.len(), 2);
        assert!(buf[0].contains("second"), "expected 'second', got: {}", buf[0]);
        assert!(buf[1].contains("third"), "expected 'third', got: {}", buf[1]);
    }

    #[test]
    fn test_log_layer_format() {
        let (buffer, subscriber) = make_layer_and_buffer(100);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "voxmux", "hello");
        });
        let buf = buffer.lock().unwrap();
        assert_eq!(buf.len(), 1);
        assert_eq!(buf[0], "[INFO] voxmux: hello");
    }
}
