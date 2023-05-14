use std::sync::Arc;

use crossbeam::atomic::AtomicCell;
use dashmap::DashMap;

use crate::{
    ingest::SinkId,
    store::{FromId, Id, Insert, Store},
};

#[derive(Debug, Default)]
pub struct TrackStore {
    tracks: DashMap<TrackId, Track>,
}

pub type Track = Arc<InternalTrack>;
pub type TrackId = Id<Track>;

#[derive(Debug, Clone)]
pub struct InternalTrack {
    pub id: TrackId,

    state: Arc<AtomicCell<TrackState>>,
}

/// Describes if this track has been ingested or not
#[derive(Debug, Clone, Copy)]
enum TrackState {
    Inactive,
    Active { sink_id: SinkId },
}

impl InternalTrack {
    pub fn new() -> Self {
        Self {
            id: TrackId::new(),
            state: Arc::new(TrackState::Inactive.into()),
        }
    }

    pub fn sink(&self) -> Option<SinkId> {
        if let TrackState::Active { sink_id } = self.state.load() {
            Some(sink_id)
        } else {
            None
        }
    }
}

impl PartialEq for InternalTrack {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl FromId<TrackId> for Track {
    type Output = Track;

    fn from_id(store: &Store, id: &TrackId) -> Option<Self>
    where
        Self: Sized,
    {
        store.track_store.tracks.get(id).map(|x| x.value().clone())
    }
}

impl FromId<SinkId> for Track {
    type Output = Track;

    fn from_id(store: &Store, id: &SinkId) -> Option<Self>
    where
        Self: Sized,
    {
        store
            .track_store
            .tracks
            .iter()
            .find(|x| x.sink().filter(|s| s == id).is_some())
            .map(|x| x.value().clone())
    }
}

impl Insert for Track {
    fn insert_into_store(self, store: &Store) {
        store.track_store.tracks.insert(self.id, self);
    }
}
