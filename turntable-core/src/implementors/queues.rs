use std::collections::VecDeque;

use parking_lot::Mutex;

use crate::{BoxedQueueItem, Queue, QueueItem, QueueNotifier};

/// A linear queue of items.
pub struct LinearQueue<T> {
    notifier: QueueNotifier,

    history: Mutex<Vec<T>>,
    items: Mutex<VecDeque<T>>,
}

impl<T> LinearQueue<T> {
    pub fn push(&self, item: T) {
        self.items.lock().push_back(item);
        self.notifier.notify();
    }
}

impl<T> Queue for LinearQueue<T>
where
    T: QueueItem + Clone,
{
    fn new(notifier: QueueNotifier) -> Self {
        Self {
            notifier,
            history: Default::default(),
            items: Default::default(),
        }
    }

    fn peek(&self) -> Vec<BoxedQueueItem> {
        self.items
            .lock()
            .iter()
            .map(|i| BoxedQueueItem::new(i.clone()))
            .collect()
    }

    fn next(&self) {
        let mut items = self.items.lock();

        if let Some(item) = items.pop_front() {
            self.history.lock().push(item);
        }

        self.notifier.notify();
    }

    fn previous(&self) {
        let mut items = self.items.lock();
        let mut history = self.history.lock();

        if let Some(item) = history.pop() {
            items.push_front(item);
        }

        self.notifier.notify();
    }

    fn reset(&self) {
        let mut items = self.items.lock();
        let mut history = self.history.lock();

        for item in history.drain(..) {
            items.push_front(item);
        }

        self.notifier.notify();
    }

    fn skip(&self, id: &str) {
        let mut items = self.items.lock();
        items.retain(|item| item.item_id() != id);
    }
}
