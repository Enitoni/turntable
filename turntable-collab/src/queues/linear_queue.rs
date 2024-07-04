use std::collections::VecDeque;

use parking_lot::Mutex;
use turntable_core::{BoxedQueueItem, Queue, QueueItem, QueueNotifier, SinkId};

use crate::{PrimaryKey, Track};

#[derive(Clone)]
pub struct LinearQueueItem {
    pub user_id: PrimaryKey,
    pub track: Track,
}

/// A linear queue of items.
pub struct LinearQueue {
    notifier: QueueNotifier,

    history: Mutex<Vec<LinearQueueItem>>,
    items: Mutex<VecDeque<LinearQueueItem>>,
}

impl LinearQueue {
    pub fn push(&self, item: Track, user_id: PrimaryKey) {
        let item = LinearQueueItem {
            user_id,
            track: item,
        };

        self.items.lock().push_back(item);
        self.notifier.notify();
    }

    /// Get a track by sink id, if it exists
    pub fn get_by_sink_id(&self, sink_id: SinkId) -> Option<LinearQueueItem> {
        self.items
            .lock()
            .iter()
            .find(|q| q.track.sink_id() == Some(sink_id))
            .cloned()
    }

    /// Gets all the tracks + history
    pub fn tracks(&self) -> (Vec<LinearQueueItem>, Vec<LinearQueueItem>) {
        let items: Vec<_> = self.items.lock().iter().cloned().collect();
        let history: Vec<_> = self.history.lock().iter().cloned().collect();

        (items, history)
    }
}

impl Queue for LinearQueue {
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
            .map(|q| BoxedQueueItem::new(q.track.clone()))
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
        items.retain(|item| item.track.item_id() != id);
    }
}
