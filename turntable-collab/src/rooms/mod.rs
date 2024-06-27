mod connection;
mod room;

use std::sync::Arc;

use crate::{CollabContext, Database, DatabaseError, NewRoom};

pub use connection::*;
pub use room::*;

use thiserror::Error;
use turntable_core::Ingestion;

pub struct RoomManager<I, Db> {
    context: CollabContext<I, Db>,
}

#[derive(Debug, Error)]
pub enum RoomError {
    #[error("Room is not active")]
    RoomNotActive,
    #[error("User is not a member of this room")]
    UserNotInRoom,
}

impl<I, Db> RoomManager<I, Db>
where
    I: Ingestion,
    Db: Database,
{
    pub fn new(context: &CollabContext<I, Db>) -> Self {
        Self {
            context: context.clone(),
        }
    }

    /// Restores the rooms from the database on init
    pub async fn restore(&self) -> Result<(), DatabaseError> {
        let rooms: Vec<_> = self
            .context
            .database
            .list_rooms()
            .await?
            .into_iter()
            .map(|r| (r.id, Room::new(&self.context, r)))
            .collect();

        for (id, room) in rooms {
            self.context.rooms.insert(id, room.into());
        }

        Ok(())
    }

    /// Creates a new room
    pub async fn create_room(&self, new_room: NewRoom) -> Result<Arc<Room<I, Db>>, DatabaseError> {
        let room_data = self.context.database.create_room(new_room).await?;
        let room = Arc::new(Room::new(&self.context, room_data));

        self.context.rooms.insert(room.id(), room.clone());

        Ok(room)
    }

    /// Get all rooms in memory
    pub fn list_all(&self) -> Vec<Arc<Room<I, Db>>> {
        self.context.rooms.iter().map(|r| r.clone()).collect()
    }
}
