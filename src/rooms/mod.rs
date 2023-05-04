use std::sync::{Arc, Weak};

use dashmap::DashMap;

mod room;
pub use room::*;

use crate::{db::Database, util::ApiError};

#[derive(Debug)]
pub struct RoomManager {
    me: Weak<RoomManager>,
    rooms: DashMap<RoomId, Room>,
}

impl RoomManager {
    pub fn new() -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            rooms: Default::default(),
        })
    }

    pub async fn init(&self, db: &Database) -> Result<(), ApiError> {
        let rooms = Room::all(db, self.me.clone()).await?;

        for room in rooms {
            self.rooms.insert(room.id.clone(), room);
        }

        Ok(())
    }

    pub(self) fn notify_disconnect(&self, room_id: &RoomId, id: ConnectionId) {
        let room = self.rooms.get(room_id).expect("room exists");
        room.disconnect(id);
    }
}
