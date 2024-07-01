mod connection;
mod room;

use std::sync::Arc;

use crate::{
    util::random_string, CollabContext, Database, DatabaseError, NewRoom, NewStreamKey, PrimaryKey,
    StreamKeyData,
};

pub use connection::*;
use futures_util::TryFutureExt;
pub use room::*;

use thiserror::Error;
use turntable_core::Ingestion;

pub struct RoomManager<I, Db> {
    context: CollabContext<I, Db>,
}

#[derive(Debug, Error)]
pub enum RoomError {
    #[error("Room does not exist")]
    RoomNotFound,
    #[error("Room is not active")]
    RoomNotActive,
    #[error("User is not a member of this room")]
    UserNotInRoom,
    #[error("User does not own this stream key")]
    StreamKeyNotOwn,
    #[error("Stream key does not exist")]
    StreamKeyNotFound,
    #[error(transparent)]
    Database(DatabaseError),
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

    /// Returns a room by id if it exists
    pub fn room_by_id(&self, room_id: PrimaryKey) -> Result<Arc<Room<I, Db>>, RoomError> {
        self.context
            .rooms
            .get(&room_id)
            .map(|r| r.clone())
            .ok_or(RoomError::RoomNotFound)
    }

    /// Get all rooms in memory
    pub fn list_all(&self) -> Vec<Arc<Room<I, Db>>> {
        self.context.rooms.iter().map(|r| r.clone()).collect()
    }

    /// Get a list of stream keys by `user_id` and `room_id`
    pub async fn list_stream_keys(
        &self,
        user_id: PrimaryKey,
        room_id: PrimaryKey,
    ) -> Result<Vec<StreamKeyData>, DatabaseError> {
        self.context
            .database
            .list_stream_keys(room_id, user_id)
            .await
    }

    /// Creates a new stream key for a room and user
    pub async fn create_stream_key(
        &self,
        room_id: PrimaryKey,
        user_id: PrimaryKey,
        source: String,
    ) -> Result<StreamKeyData, RoomError> {
        // Ensure room exists
        let room = self.room_by_id(room_id)?;
        // Ensure user is a member of the room
        let _ = room.member_by_user_id(user_id)?;

        let token = random_string(32);

        self.context
            .database
            .create_stream_key(NewStreamKey {
                token,
                room_id,
                user_id,
                source,
            })
            .map_err(RoomError::Database)
            .await
    }

    /// Connects to a room and returns a connection handle using a stream key token
    pub async fn connect(
        &self,
        for_user_id: PrimaryKey,
        token: String,
    ) -> Result<RoomConnectionHandle<I, Db>, RoomError> {
        let stream_key = self
            .context
            .database
            .stream_key_by_token(&token)
            .await
            .map_err(|e| match e {
                DatabaseError::NotFound {
                    resource: _,
                    identifier: _,
                } => RoomError::StreamKeyNotFound,
                e => RoomError::Database(e),
            })?;

        if stream_key.user_id != for_user_id {
            return Err(RoomError::StreamKeyNotOwn);
        }

        let room = self.room_by_id(stream_key.room_id)?;
        let handle = room.connect(stream_key.user_id, stream_key.source)?;

        Ok(handle)
    }

    /// Deletes a stream key
    pub async fn delete_stream_key(&self, key_id: PrimaryKey) -> Result<(), DatabaseError> {
        self.context.database.delete_stream_key(key_id).await
    }
}