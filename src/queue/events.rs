use crate::{
    events::{Filter, IntoEvent},
    VinylEvent,
};

use super::{Item, ItemId, QueueId};

#[derive(Debug, Clone)]
pub enum QueueEvent {
    Update {
        queue: QueueId,
        new_items: Vec<Item>,
    },
    Advance {
        queue: QueueId,
        item: ItemId,
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
