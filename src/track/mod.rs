use std::sync::Arc;

use crossbeam::atomic::AtomicCell;
use dashmap::DashMap;
use serde::Serialize;

use crate::{
    audio::Input,
    ingest::{Ingestion, InputError, ProbeResult, SinkId},
    store::{FromId, Id, Insert, Store},
};

#[derive(Debug, Default)]
pub struct TrackStore {
    tracks: DashMap<TrackId, Track>,
}

pub type Track = Arc<InternalTrack>;
pub type TrackId = Id<Track>;

#[derive(Debug, Clone, Serialize)]
pub struct InternalTrack {
    pub id: TrackId,
    pub metadata: Metadata,

    #[serde(skip)]
    input: Input,

    #[serde(skip)]
    state: Arc<AtomicCell<TrackState>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Metadata {
    pub title: String,
    pub artist: String,

    pub canonical: String,
    pub source: String,

    pub duration: f32,
    pub artwork: Option<String>,
}

/// Describes if this track has been ingested or not
#[derive(Debug, Clone, Copy)]
enum TrackState {
    Inactive,
    Active { sink_id: SinkId, probe: ProbeResult },
    Error,
}

impl InternalTrack {
    pub fn new(input: Input) -> Self {
        let metadata = input.metadata();

        Self {
            input,
            metadata,
            id: TrackId::new(),
            state: Arc::new(TrackState::Inactive.into()),
        }
    }

    pub fn sink(&self) -> Option<SinkId> {
        if let TrackState::Active { sink_id, probe: _ } = self.state.load() {
            Some(sink_id)
        } else {
            None
        }
    }

    /// Returns true if the track is suitable in a playback context
    pub fn suitable(&self) -> bool {
        !matches!(self.state.load(), TrackState::Error)
    }

    pub fn ensure_activation(&self, ingestion: &Ingestion) -> Result<(), InputError> {
        if let TrackState::Inactive = self.state.load() {
            return match self.activate(ingestion) {
                Ok(_) => Ok(()),
                Err(e) => {
                    self.state.store(TrackState::Error);
                    Err(e)
                }
            };
        }

        Ok(())
    }

    fn activate(&self, ingestion: &Ingestion) -> Result<(), InputError> {
        let loader = self.input.loader()?;
        let result = loader.probe().ok_or(InputError::Unknown)?;

        let sink = ingestion.add(result, loader);

        self.state.store(TrackState::Active {
            sink_id: sink,
            probe: result,
        });

        Ok(())
    }

    #[cfg(test)]
    pub fn mock(title: &str) -> Track {
        let meta = Metadata {
            title: title.to_string(),
            artist: "artist".to_string(),
            canonical: "".to_string(),
            source: "mock".to_string(),
            duration: 0.,
            artwork: None,
        };

        InternalTrack::new(Input::Empty(meta)).into()
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
