use crate::{
    events::{Filter, IntoEvent},
    VinylEvent,
};

use super::PlayerId;

#[derive(Debug, Clone)]
pub enum AudioEvent {
    /// The player is now playing a new sink
    Next { player: PlayerId },
    /// The player advanced ahead
    Time {
        player: PlayerId,
        total_offset: usize,
        offset: usize,
    },
}

impl IntoEvent<VinylEvent> for AudioEvent {
    fn into_event(self) -> VinylEvent {
        VinylEvent::Audio(self)
    }
}

impl Filter<VinylEvent> for AudioEvent {
    fn filter(event: VinylEvent) -> Option<Self> {
        match event {
            VinylEvent::Audio(x) => Some(x),
            _ => None,
        }
    }
}
