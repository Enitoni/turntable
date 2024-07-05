use async_trait::async_trait;
use parking_lot::Mutex;
use std::{error::Error, sync::Arc};
use turntable_core::{BoxedLoadable, Id, QueueItem, SinkId};

use crate::{input::Input, Metadata};

pub type TrackId = Id<Track>;

/// A single track in a queue
#[derive(Debug, Clone)]
pub struct Track {
    pub id: TrackId,
    pub metadata: Metadata,

    input: Arc<Input>,
    state: Arc<Mutex<TrackState>>,
}

#[derive(Debug, Default, Clone)]
enum TrackState {
    #[default]
    Inactive,
    Active(SinkId),
    Error(String),
}

#[async_trait]
impl QueueItem for Track {
    fn length(&self) -> Option<f32> {
        self.input.length()
    }

    fn item_id(&self) -> String {
        self.id.to_string()
    }

    fn register_sink(&self, sink_id: SinkId) {
        *self.state.lock() = TrackState::Active(sink_id);
    }

    fn sink_id(&self) -> Option<SinkId> {
        match *self.state.lock() {
            TrackState::Active(sink_id) => Some(sink_id),
            _ => None,
        }
    }

    async fn loadable(&self) -> Result<BoxedLoadable, Box<dyn Error>> {
        Ok(self.input.loadable().await?)
    }
}

impl From<Input> for Track {
    fn from(input: Input) -> Self {
        Self {
            state: Default::default(),
            metadata: input.metadata(),
            input: Arc::new(input),
            id: TrackId::new(),
        }
    }
}
