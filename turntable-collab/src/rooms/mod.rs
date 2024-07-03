mod connection;
mod room;

use std::sync::Arc;

use crate::{
    util::random_string, CollabContext, Database, DatabaseError, NewRoom, NewRoomInvite,
    NewRoomMember, NewStreamKey, PrimaryKey, RoomInviteData, StreamKeyData,
};

pub use connection::*;
use futures_util::TryFutureExt;
pub use room::*;

use thiserror::Error;

pub struct RoomManager {
    context: CollabContext,
}

#[derive(Debug, Error)]
pub enum RoomError {
    #[error("Room {0} does not exist")]
    RoomNotFound(String),
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

impl RoomManager {
    pub fn new(context: &CollabContext) -> Self {
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
    pub async fn create_room(&self, new_room: NewRoom) -> Result<Arc<Room>, DatabaseError> {
        let room_data = self.context.database.create_room(new_room).await?;
        let room = Arc::new(Room::new(&self.context, room_data));

        self.context.rooms.insert(room.id(), room.clone());

        Ok(room)
    }

    /// Returns a room by id if it exists
    pub fn room_by_id(&self, room_id: PrimaryKey) -> Result<Arc<Room>, RoomError> {
        self.context
            .rooms
            .get(&room_id)
            .map(|r| r.clone())
            .ok_or(RoomError::RoomNotFound(room_id.to_string()))
    }

    /// Returns a room by slug if it exists
    pub fn room_by_slug(&self, slug: &str) -> Result<Arc<Room>, RoomError> {
        self.context
            .rooms
            .iter()
            .find(|r| r.data().slug == slug)
            .map(|r| r.clone())
            .ok_or(RoomError::RoomNotFound(slug.to_string()))
    }

    /// Get all rooms in memory
    pub fn list_all(&self) -> Vec<Arc<Room>> {
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
    pub async fn connect(&self, token: String) -> Result<RoomConnectionHandle, RoomError> {
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

        let room = self.room_by_id(stream_key.room_id)?;
        let handle = room.connect(stream_key.user_id, stream_key.source)?;

        Ok(handle)
    }

    /// Deletes a stream key
    pub async fn delete_stream_key(&self, key_id: PrimaryKey) -> Result<(), DatabaseError> {
        self.context.database.delete_stream_key(key_id).await
    }

    /// Creates an invite for a room
    pub async fn create_invite(
        &self,
        inviter_id: PrimaryKey,
        for_room: PrimaryKey,
    ) -> Result<RoomInviteData, RoomError> {
        // Ensure room exists
        let room = self.room_by_id(for_room)?;
        // Ensure user is a member of the room
        let _ = room.member_by_user_id(inviter_id)?;

        let token = random_string(32);

        self.context
            .database
            .create_room_invite(NewRoomInvite {
                room_id: for_room,
                user_id: inviter_id,
                token,
            })
            .await
            .map_err(RoomError::Database)
    }

    /// Gets an invite by token
    pub async fn invite_by_token(&self, token: String) -> Result<RoomInviteData, DatabaseError> {
        self.context.database.room_invite_by_token(&token).await
    }

    /// Adds a user as a member to a room by consuming an invite
    pub async fn add_member_with_invite(
        &self,
        user_id: PrimaryKey,
        token: String,
    ) -> Result<(), RoomError> {
        let invite = self
            .context
            .database
            .room_invite_by_token(&token)
            .await
            .map_err(RoomError::Database)?;

        let room = self.room_by_id(invite.room.id)?;

        let member = self
            .context
            .database
            .create_room_member(NewRoomMember {
                owner: false,
                room_id: room.id(),
                user_id,
            })
            .map_err(RoomError::Database)
            .await?;

        room.add_member(member);

        self.context
            .database
            .delete_room_invite(invite.id)
            .map_err(RoomError::Database)
            .await?;

        Ok(())
    }
}
