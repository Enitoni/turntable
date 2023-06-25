use std::cell::RefCell;

use crossbeam::atomic::AtomicCell;
use parking_lot::Mutex;
use serde::Serialize;
use surrealdb::sql::Thing;

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
    robin: RoundRobin,

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
            robin: Default::default(),
            items: Default::default(),
        }
    }

    pub(self) fn tracks_to_play(&self) -> Vec<Track> {
        let current_index = self.current_index();

        self.items
            .lock()
            .iter()
            .skip(current_index)
            .filter(|t| t.track.suitable())
            .take(3)
            .map(|x| x.track.clone())
            .collect()
    }

    pub fn add(&self, submitter: &User, tracks: Vec<Track>) {
        self.robin.add(submitter, tracks);

        if self.current_item.load() == Id::none() {
            self.advance_index(0);
        }

        self.update();
    }

    pub fn next(&self) -> Option<QueueItem> {
        self.advance_index(1);
        self.update();
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
        *self.items.lock() = self.robin.items();
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
    fn new(owner: User, ordering: OrderStrategy) -> Self {
        Self {
            id: Id::new(),
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

    fn next(&self) -> Option<QueueItem> {
        let mut entries = self.entries.lock();
        let next = entries.drain(1..).next();

        if let Some(next) = next {
            let (item, entry) = next.consume_one(self.owner.id.clone());

            if let Some(entry) = entry {
                entries.push(entry);
            }

            return Some(item);
        }

        None
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

    /// Consumes one item from the entry, returning the item and entry if the entry has more items
    fn consume_one(self, submitter: UserId) -> (QueueItem, Option<Entry>) {
        match self {
            Entry::Single(track, id) => (
                QueueItem {
                    id,
                    submitter,
                    track: track.clone(),
                },
                None,
            ),
            Entry::Multiple(mut items) => {
                let item = items.drain(1..).next().expect("items is not empty");
                let new_length = items.len();

                let item = QueueItem {
                    id: item.1,
                    submitter,
                    track: item.0,
                };

                if new_length > 1 {
                    (item, Some(Entry::Multiple(items)))
                } else {
                    let last_item = items.into_iter().next().expect("items is not empty");
                    (item, Some(Entry::Single(last_item.0, last_item.1)))
                }
            }
        }
    }
}

#[derive(Debug, Serialize, Clone)]
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
            submitters: queue.robin.submitters(),
        }
    }
}

#[derive(Debug, Default)]
pub struct RoundRobin {
    history: Mutex<Vec<QueueItem>>,
    queues: Mutex<Vec<SubQueue>>,
}

impl RoundRobin {
    fn next(&self) {
        let submitters = self.ordered_submitters();
        let submitter = submitters.get(0);
        let queues = self.queues.lock();

        let queue_to_consume =
            submitter.and_then(|s| queues.iter().find(|q| q.owner.id == s.clone()));

        let next_item = queue_to_consume.and_then(|q| q.next());

        if let Some(next_item) = next_item {
            self.history.lock().push(next_item);
        }
    }

    fn calculate(&self) -> Vec<QueueItem> {
        let submitters = self.ordered_submitters();
        let queues = self.queues.lock();

        let iterators = submitters
            .into_iter()
            .flat_map(|s| queues.iter().find(|q| q.owner.id == s))
            .map(|x| RefCell::new(x.to_items().into_iter()))
            .collect::<Vec<_>>();

        let iterations: usize = queues.iter().map(|x| x.len()).sum();

        iterators
            .iter()
            .cycle()
            .take(iterations)
            .flat_map(|i| i.borrow_mut().next())
            .flatten()
            .collect()
    }

    /// Returns an ordered list of submitters based on priority
    fn ordered_submitters(&self) -> Vec<Thing> {
        let history = self.history.lock();
        let queues = self.queues.lock();

        let mut submitters: Vec<_> = queues.iter().map(|q| q.owner.id.clone()).collect();

        let recent_submitters: Vec<_> = history
            .get(..submitters.len())
            .map(|slice| slice.iter().map(|s| &s.submitter).collect())
            .unwrap_or_default();

        let prioritized_submitter = submitters
            .iter()
            .find(|s| !recent_submitters.contains(s))
            .or_else(|| recent_submitters.get(0).copied());

        let starting_queue_index = queues
            .iter()
            .enumerate()
            .find_map(|(i, q)| {
                prioritized_submitter
                    .filter(|x| *x == &q.owner.id)
                    .map(|_| i)
            })
            .unwrap_or_default();

        submitters.rotate_left(starting_queue_index);
        submitters
    }

    fn items(&self) -> Vec<QueueItem> {
        let calculated = self.calculate();
        let history = self.history.lock();
        let mut result = vec![];

        result.extend(history.iter().cloned());
        result.extend(calculated);

        result
    }

    fn add(&self, user: &User, tracks: Vec<Track>) {
        self.ensure_sub_queue(user);

        let queues = self.queues.lock();
        let queue = queues
            .iter()
            .find(|q| q.owner.id == user.id)
            .expect("queue exists after it was ensured");

        queue.add(Entry::new(tracks));
    }

    fn ensure_sub_queue(&self, user: &User) {
        let mut queues = self.queues.lock();
        let queue_exists = queues.iter().any(|q| q.owner.id == user.id);

        if !queue_exists {
            let new_queue = SubQueue::new(user.clone(), OrderStrategy::Interleave);
            queues.push(new_queue);
        }
    }

    fn submitters(&self) -> Vec<User> {
        self.queues.lock().iter().map(|q| q.owner.clone()).collect()
    }
}
