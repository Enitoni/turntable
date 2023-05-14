use crate::{
    events::{Filter, IntoEvent},
    VinylEvent,
};

use super::{QueueId, QueueItem};

#[derive(Debug, Clone)]
pub enum QueueEvent {
    Update {
        queue: QueueId,
        new_items: Vec<QueueItem>,
    },
    Advance {
        queue: QueueId,
        item: QueueItem,
    },
}

impl IntoEvent<VinylEvent> for QueueEvent {
    fn into_event(self) -> VinylEvent {
        VinylEvent::Queue(self)
    }
}

impl Filter<VinylEvent> for QueueEvent {
    fn filter(event: VinylEvent) -> Option<Self> {
        match event {
            VinylEvent::Queue(x) => Some(x),
            _ => None,
        }
    }
}
