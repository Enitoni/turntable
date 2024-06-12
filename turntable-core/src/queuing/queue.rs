use crate::{BoxedQueueItem, QueueNotifier};

/// Represents a type that acts as a consumable queue.
pub trait Queue
where
    Self: 'static + Sync + Send,
{
    /// Instantiates this queue.
    /// * `notifier` - The notifier that should be used to notify when a queue updates.
    fn new(notifier: QueueNotifier) -> Self
    where
        Self: Sized;

    /// Returns the current and next items in the queue.
    fn peek(&self) -> &[BoxedQueueItem];

    /// Advances the queue forward by one item.
    fn next(&self);

    /// Backtracks the queue by one item.
    fn previous(&self);

    /// Resets the queue back to the beginning.
    fn reset(&self);
}

/// [Queue] trait object.
pub struct BoxedQueue(Box<dyn Queue>);

impl Queue for BoxedQueue {
    fn new(_: QueueNotifier) -> Self {
        panic!("Queue::new() should not be called on a BoxedQueue");
    }

    fn peek(&self) -> &[BoxedQueueItem] {
        self.0.peek()
    }

    fn next(&self) {
        self.0.next()
    }

    fn previous(&self) {
        self.0.previous()
    }

    fn reset(&self) {
        self.0.reset()
    }
}
