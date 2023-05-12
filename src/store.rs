use std::fmt::{Debug, Display};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::sync::Arc;

use crate::audio::Playback;
use crate::ingest::Ingestion;
use crate::track::TrackStore;
use crate::util::ID_COUNTER;
use crate::EventEmitter;

/// A store containing all top-level data structures
#[derive(Debug)]
pub struct Store {
    pub track_store: TrackStore,
    pub playback: Arc<Playback>,
    pub ingestion: Arc<Ingestion>,
}

impl Store {
    pub fn new(emitter: EventEmitter) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            playback: Playback::new(me.clone(), emitter.clone()).into(),
            ingestion: Ingestion::new(emitter).into(),
            track_store: Default::default(),
        })
    }

    /// Gets a resource based on the id
    pub fn get<I, T>(&self, id: &I) -> Option<T>
    where
        T: FromId<I>,
    {
        T::from_id(self, id)
    }

    /// Insert a resource into the store
    pub fn insert<T>(&self, resource: T)
    where
        T: Insert,
    {
        T::insert_into_store(resource, self);
    }
}

pub struct Id<T> {
    value: u64,
    kind: PhantomData<T>,
}

impl<T> Id<T> {
    pub fn new() -> Self {
        Self {
            value: ID_COUNTER.fetch_add(1),
            kind: PhantomData,
        }
    }

    pub fn none() -> Self {
        Self {
            value: 0,
            kind: PhantomData,
        }
    }
}

impl<T> Default for Id<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        Id {
            kind: self.kind,
            value: self.value,
        }
    }
}

impl<T> Copy for Id<T> {}
impl<T> Eq for Id<T> {}

impl<T> Debug for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl<T> Display for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state)
    }
}

pub trait FromId<I> {
    fn from_id(store: &Store, id: &I) -> Option<Self>
    where
        Self: Sized;
}

pub trait Insert {
    fn insert_into_store(self, store: &Store);
}
