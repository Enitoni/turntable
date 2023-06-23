use super::RoomId;
use crate::{
    auth::{User, UserId},
    events::{Filter, IntoEvent},
    VinylEvent,
};

#[derive(Debug, Clone)]
pub enum RoomEvent {
    /// A new user connected the room stream
    UserEnteredRoom { user: User, room: RoomId },
    /// A user disconnected from the room stream
    UserLeftRoom { user: UserId, room: RoomId },
}

impl IntoEvent<VinylEvent> for RoomEvent {
    fn into_event(self) -> VinylEvent {
        VinylEvent::Room(self)
    }
}

impl Filter<VinylEvent> for RoomEvent {
    fn filter(event: VinylEvent) -> Option<Self> {
        match event {
            VinylEvent::Room(x) => Some(x),
            _ => None,
        }
    }
}
