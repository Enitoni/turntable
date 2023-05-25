use std::cell::RefCell;

use crossbeam::atomic::AtomicCell;
use parking_lot::Mutex;
use serde::Serialize;

use crate::{
    auth::{User, UserId},
    store::Id,
    track::Track,
};

mod events;
mod store;

pub type QueueId = Id<Queue>;
pub type SubQueueId = Id<SubQueue>;
pub type QueueItemId = Id<QueueItem>;

pub use events::*;
pub use store::*;

/// A queue, belonging to a room
#[derive(Debug)]
pub struct Queue {
    id: QueueId,
    sub_queues: Mutex<Vec<SubQueue>>,

    /// The current track playing
    current_item: AtomicCell<QueueItemId>,

    /// The calculated list of queue items
    items: Mutex<Vec<QueueItem>>,
}

/// An item  in the queue
#[derive(Debug, Clone, Serialize)]
pub struct QueueItem {
    id: QueueItemId,
    submitter: UserId,
    track: Track,
}

/// A sub queue allows a queue to be non-destructive and dynamic
#[derive(Debug)]
pub struct SubQueue {
    id: SubQueueId,
    parent: QueueId,
    owner: User,
    ordering: OrderStrategy,
    entries: Mutex<Vec<Entry>>,
}

/// Describes how items from a sub queue should be prioritized
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrderStrategy {
    /// This sub queue will be interleaved next to other sub-queues
    Interleave,
    /// This sub-queue is only used if there is no other sub-queues with content
    Fallback,
}

#[derive(Debug)]
pub enum Entry {
    Single(Track, QueueItemId),
    Multiple(Vec<(Track, QueueItemId)>),
}

impl Queue {
    pub(self) fn new() -> Self {
        Self {
            id: Id::new(),
            current_item: Id::none().into(),
            sub_queues: Default::default(),
            items: Default::default(),
        }
    }

    pub(self) fn has_sub_queue(&self, sub_queue: SubQueueId) -> bool {
        self.sub_queues.lock().iter().any(|s| s.id == sub_queue)
    }

    pub(self) fn tracks_to_play(&self) -> Vec<Track> {
        let current_index = self.current_index();

        self.items
            .lock()
            .iter()
            .skip(current_index)
            .take(3)
            .map(|x| x.track.clone())
            .collect()
    }

    pub fn create_sub_queue(&self, owner: User, ordering: OrderStrategy) -> SubQueueId {
        let new_sub_queue = SubQueue::new(self.id, owner, ordering);
        let id = new_sub_queue.id;

        self.sub_queues.lock().push(new_sub_queue);
        id
    }

    pub fn add(&self, sub_queue: SubQueueId, tracks: Vec<Track>) {
        let guard = self.sub_queues.lock();

        let queue = guard
            .iter()
            .find(|q| q.id == sub_queue)
            .expect("sub queue exists");

        let item = QueueItem {
            id: Id::new(),
            submitter: queue.owner.id.clone(),
            track: tracks.get(0).expect("first track").clone(),
        };

        self.items.lock().push(item);
    }

    pub fn next(&self) -> Option<QueueItem> {
        self.advance_index(1);
        self.current_item()
    }

    pub fn items(&self) -> Vec<QueueItem> {
        self.items.lock().clone()
    }

    pub fn current_item(&self) -> Option<QueueItem> {
        let current_index = self.current_index();
        self.items.lock().get(current_index).cloned()
    }

    /// Gets the index based on the current item being played
    fn current_index(&self) -> usize {
        let current_item = self.current_item.load();

        self.items
            .lock()
            .iter()
            .enumerate()
            .find_map(|(idx, i)| (i.id == current_item).then_some(idx))
            .unwrap_or_default()
    }

    fn advance_index(&self, amount: usize) {
        let current_index = self.current_index();
        let new_item = self.item_at(current_index + amount);

        self.current_item.store(new_item.unwrap_or_default());
    }

    /// Returns the index in a cyclic manner
    fn index_at(&self, index: usize) -> usize {
        let items = self.items.lock().len();
        index.checked_rem_euclid(items).unwrap_or_default()
    }

    fn item_at(&self, index: usize) -> Option<QueueItemId> {
        let index = self.index_at(index);
        self.items.lock().get(index).map(|i| i.id)
    }

    /// Should be called whenever the queue changes
    fn update(&self) {
        //let sub_queues = self.sub_queues.lock();
        //let updated_items = Self::collect(&sub_queues);

        //*self.items.lock() = updated_items;
    }

    /// Gets the interleaved items from each interleaving sub-queue
    fn collect_interleaved(queues: &[SubQueue]) -> Vec<QueueItem> {
        let iterations: usize = queues
            .iter()
            .filter(|x| x.ordering == OrderStrategy::Interleave)
            .map(|x| x.len())
            .sum();

        // We get iterators here so we can call next on each one
        let iterators = queues
            .iter()
            .map(|x| RefCell::new(x.to_items().into_iter()))
            .collect::<Vec<_>>();

        iterators
            .iter()
            .cycle()
            .take(iterations)
            .flat_map(|i| i.borrow_mut().next())
            .flatten()
            .collect()
    }

    /// Gets the items from each fallback sub-queue
    fn collect_fallback(queues: &[SubQueue]) -> Vec<QueueItem> {
        queues
            .iter()
            .filter(|x| x.ordering == OrderStrategy::Fallback)
            .flat_map(|x| x.to_items())
            .flatten()
            .collect()
    }

    /// Collects all sub-queues into a queue of items
    fn collect(queues: &[SubQueue]) -> Vec<QueueItem> {
        let mut interleaving = Self::collect_interleaved(queues);
        let mut fallback = Self::collect_fallback(queues);

        interleaving.append(&mut fallback);
        interleaving
    }
}

impl SubQueue {
    fn new(parent: QueueId, owner: User, ordering: OrderStrategy) -> Self {
        Self {
            id: Id::new(),
            parent,
            owner,
            ordering,
            entries: Default::default(),
        }
    }

    fn add(&self, entry: Entry) {
        self.entries.lock().push(entry);
    }

    fn len(&self) -> usize {
        self.entries.lock().len()
    }

    fn to_items(&self) -> Vec<Vec<QueueItem>> {
        self.entries
            .lock()
            .iter()
            .map(|x| x.to_items(self.owner.id.clone()))
            .collect()
    }
}

impl Entry {
    fn new(tracks: Vec<Track>) -> Self {
        let with_ids: Vec<_> = tracks.into_iter().map(|t| (t, Id::new())).collect();

        if with_ids.len() == 1 {
            let (track, id) = with_ids.into_iter().next().unwrap();
            Self::Single(track, id)
        } else {
            Self::Multiple(with_ids)
        }
    }

    fn to_items(&self, submitter: UserId) -> Vec<QueueItem> {
        match self {
            Entry::Single(track, id) => vec![QueueItem {
                id: *id,
                submitter,
                track: track.clone(),
            }],
            Entry::Multiple(x) => x
                .clone()
                .into_iter()
                .map(|(track, id)| QueueItem {
                    id,
                    track,
                    submitter: submitter.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializedQueue {
    id: QueueId,
    items: Vec<QueueItem>,
    current_item: QueueItemId,
    submitters: Vec<User>,
}

impl SerializedQueue {
    pub fn new(queue: &Queue) -> Self {
        Self {
            id: queue.id,
            current_item: queue.current_item.load(),
            items: queue.items(),
            submitters: queue
                .sub_queues
                .lock()
                .iter()
                .map(|s| s.owner.clone())
                .collect(),
        }
    }
}
