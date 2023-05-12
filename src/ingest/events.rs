use crate::{
    events::{Filter, IntoEvent},
    VinylEvent,
};

use super::{SinkId, SinkLength};

#[derive(Debug, Clone)]
pub enum IngestionEvent {
    Finished {
        sink: SinkId,
        total: usize,
    },
    Loading {
        sink: SinkId,
        amount: usize,
    },
    Loaded {
        sink: SinkId,
        amount: usize,
        expected: SinkLength,
    },
}

impl IntoEvent<VinylEvent> for IngestionEvent {
    fn into_event(self) -> VinylEvent {
        VinylEvent::Ingestion(self)
    }
}

impl Filter<VinylEvent> for IngestionEvent {
    fn filter(event: VinylEvent) -> Option<Self> {
        match event {
            VinylEvent::Ingestion(x) => Some(x),
            _ => None,
        }
    }
}
