use std::sync::{Arc, Weak};

use dashmap::DashMap;

mod room;
mod router;

pub use room::*;
pub use router::router;

use crate::{auth::User, db::Database, events::Events, util::ApiError};

#[derive(Debug)]
pub struct RoomManager {
    events: Events,
    me: Weak<RoomManager>,
    rooms: DashMap<RoomId, Room>,
}

impl RoomManager {
    pub fn new(events: Events) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            events,
            me: me.clone(),
            rooms: Default::default(),
        })
    }

    pub async fn init(&self, db: &Database) -> Result<(), ApiError> {
        let rooms = Room::all(db, self.events.clone(), self.me.clone()).await?;

        for room in rooms {
            self.rooms.insert(room.id.clone(), room);
        }

        Ok(())
    }

    pub async fn create_room(
        &self,
        db: &Database,
        user: &User,
        name: String,
    ) -> Result<Room, ApiError> {
        let room = Room::create(db, self.events.clone(), self.me.clone(), user, name).await?;
        self.rooms.insert(room.id.clone(), room.clone());

        Ok(room)
    }

    pub fn rooms(&self) -> Vec<Room> {
        self.rooms.iter().map(|r| r.value().clone()).collect()
    }

    pub(self) fn notify_disconnect(&self, room_id: &RoomId, id: ConnectionId) {
        let room = self.rooms.get(room_id).expect("room exists");
        room.disconnect(id);
    }
}
