use crate::BoxedQueueItem;
use std::sync::Arc;

/// Represents a type that acts as a consumable queue.
pub trait Queue
where
    Self: 'static + Sync + Send,
{
    /// Returns the current and next items in the queue.
    fn peek(&self) -> Vec<BoxedQueueItem>;

    /// Advances the queue forward by one item.
    fn next(&self);

    /// Backtracks the queue by one item.
    fn previous(&self);

    /// Resets the queue back to the beginning.
    fn reset(&self);

    /// Skip the given item. This is called when a queue item could not be ingested due to an error.
    ///
    /// Implementors are expected to remove the item from the queue without notifying.
    fn skip(&self, id: &str);
}

/// [Queue] trait object.
pub struct BoxedQueue(Box<dyn Queue>);

impl BoxedQueue {
    pub fn new<T>(queue: T) -> Self
    where
        T: Queue,
    {
        BoxedQueue(Box::new(queue))
    }
}

impl Queue for BoxedQueue {
    fn peek(&self) -> Vec<BoxedQueueItem> {
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

    fn skip(&self, id: &str) {
        self.0.skip(id)
    }
}

impl<T> Queue for Arc<T>
where
    T: Queue,
{
    fn peek(&self) -> Vec<BoxedQueueItem> {
        self.as_ref().peek()
    }

    fn next(&self) {
        self.as_ref().next()
    }

    fn previous(&self) {
        self.as_ref().previous()
    }

    fn reset(&self) {
        self.as_ref().reset()
    }

    fn skip(&self, id: &str) {
        self.as_ref().skip(id)
    }
}
