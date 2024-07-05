use std::collections::VecDeque;

use parking_lot::Mutex;
use turntable_core::{BoxedQueueItem, Queue, QueueItem, QueueNotifier, SinkId};

use crate::{events::CollabEvent, CollabContext, PrimaryKey, Track};

#[derive(Debug, Clone)]
pub struct LinearQueueItem {
    pub user_id: PrimaryKey,
    pub track: Track,
}

/// Wraps the [QueueNotifier] to also emit a collab event when the queue updates
pub struct WrappedQueueNotifier {
    pub room_id: PrimaryKey,
    pub context: CollabContext,
    pub notifier: QueueNotifier,
}

/// A linear queue of items.
pub struct LinearQueue {
    notifier: WrappedQueueNotifier,

    history: Mutex<Vec<LinearQueueItem>>,
    items: Mutex<VecDeque<LinearQueueItem>>,
}

impl LinearQueue {
    pub fn new(notifier: WrappedQueueNotifier) -> Self {
        Self {
            notifier,
            history: Default::default(),
            items: Default::default(),
        }
    }

    pub fn push(&self, item: Track, user_id: PrimaryKey) {
        let item = LinearQueueItem {
            user_id,
            track: item,
        };

        {
            self.items.lock().push_back(item);
        }

        self.notify();
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

    fn notify(&self) {
        let tracks = self.tracks();
        self.notifier.notify(tracks.0, tracks.1);
    }
}

impl Queue for LinearQueue {
    fn peek(&self) -> Vec<BoxedQueueItem> {
        self.items
            .lock()
            .iter()
            .map(|q| BoxedQueueItem::new(q.track.clone()))
            .collect()
    }

    fn next(&self) {
        {
            let mut items = self.items.lock();

            if let Some(item) = items.pop_front() {
                self.history.lock().push(item);
            }
        }

        self.notify();
    }

    fn previous(&self) {
        {
            let mut items = self.items.lock();
            let mut history = self.history.lock();

            if let Some(item) = history.pop() {
                items.push_front(item);
            }
        }

        self.notify();
    }

    fn reset(&self) {
        {
            let mut items = self.items.lock();
            let mut history = self.history.lock();

            for item in history.drain(..) {
                items.push_front(item);
            }
        }

        self.notify();
    }

    fn skip(&self, id: &str) {
        let mut items = self.items.lock();
        items.retain(|item| item.track.item_id() != id);
    }
}

impl WrappedQueueNotifier {
    fn notify(&self, items: Vec<LinearQueueItem>, history: Vec<LinearQueueItem>) {
        self.context.emit(CollabEvent::RoomQueueUpdate {
            room_id: self.room_id,
            history,
            items,
        });
        self.notifier.notify();
    }
}
