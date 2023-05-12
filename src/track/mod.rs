use std::sync::Arc;

use dashmap::DashMap;

use crate::store::{FromId, Id, Insert, Store};

#[derive(Debug)]
pub struct TrackStore {
    tracks: DashMap<TrackId, Track>,
}

pub type TrackId = Id<Track>;
pub type Track = Arc<InnerTrack>;

#[derive(Debug, Clone)]
pub struct InnerTrack {
    pub id: TrackId,
}

impl InnerTrack {
    pub fn new() -> Self {
        Self { id: TrackId::new() }
    }
}

impl PartialEq for InnerTrack {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl FromId<TrackId> for Track {
    fn from_id(store: &Store, id: &TrackId) -> Option<Self>
    where
        Self: Sized,
    {
        store.track_store.tracks.get(id).map(|x| x.value().clone())
    }
}

impl Insert for InnerTrack {
    fn insert_into_store(self, store: &Store) {
        store.track_store.tracks.insert(self.id, self.into());
    }
}
