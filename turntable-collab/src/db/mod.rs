use async_trait::async_trait;
use thiserror::Error;

mod data;
pub use data::*;

pub type Result<T> = std::result::Result<T, DatabaseError>;

#[derive(Debug, Error)]
pub enum DatabaseError {
    /// An unknown or internal error happened with the database
    #[error(transparent)]
    Internal(Box<dyn std::error::Error>),
    /// A resource already exists
    #[error("{resource} with {field} of value {value} already exists")]
    Conflict {
        /// The resource in question
        resource: String,
        /// The field that is conflicting
        field: String,
        /// The conflicting value
        value: String,
    },
    /// A resource in the database doesn't exist
    #[error("{resource}:{identifier} doesn't exist")]
    NotFound {
        resource: String,
        identifier: String,
    },
}

/// Represents a type that can fetch turntable data from a database
#[async_trait]
pub trait Database {
    async fn user_by_id(&self, user_id: PrimaryKey) -> Result<UserData>;
    async fn user_by_username(&self, username: &str) -> Result<UserData>;
    async fn create_user(&self, new_user: NewUser) -> Result<UserData>;
    async fn update_user(&self, updated_user: UpdatedUser) -> Result<UserData>;
    async fn delete_user(&self, user_id: PrimaryKey) -> Result<()>;

    async fn session_by_token(&self, token: &str) -> Result<SessionData>;
    async fn create_session(&self, new_session: NewSession) -> Result<SessionData>;
    async fn delete_session_by_token(&self, token: &str) -> Result<()>;
    async fn clear_expired_sessions(&self) -> Result<()>;

    async fn room_by_id(&self, room_id: PrimaryKey) -> Result<RoomData>;
    async fn room_by_slug(&self, slug: &str) -> Result<RoomData>;
    async fn room_invite_by_token(&self, token: &str) -> Result<RoomInviteData>;
    async fn list_rooms(&self) -> Result<Vec<RoomData>>;
    async fn create_room(&self, new_room: NewRoom) -> Result<RoomData>;
    async fn create_room_member(&self, new_member: NewRoomMember) -> Result<()>;
    async fn update_room(&self, updated_room: UpdatedRoom) -> Result<RoomData>;
    async fn delete_room(&self, room_id: PrimaryKey) -> Result<()>;
    async fn delete_room_member(&self, user_id: PrimaryKey) -> Result<()>;
    async fn create_room_invite(&self, new_room_invite: NewRoomInvite) -> Result<RoomInviteData>;

    async fn stream_key_by_token(&self, token: &str) -> Result<StreamKeyData>;
    async fn create_stream_key(&self, new_key: NewStreamKey) -> Result<StreamKeyData>;
    async fn list_stream_keys(
        &self,
        room_id: PrimaryKey,
        user_id: PrimaryKey,
    ) -> Result<Vec<StreamKeyData>>;
    async fn delete_stream_key(&self, key_id: PrimaryKey) -> Result<()>;
}

#[derive(Debug)]
pub struct NewUser {
    pub username: String,
    pub password: String,
    pub display_name: String,
}

#[derive(Debug)]
pub struct UpdatedUser {
    pub id: PrimaryKey,
    pub display_name: Option<String>,
}

#[derive(Debug)]
pub struct NewSession {
    pub token: String,
    pub user_id: PrimaryKey,
}

#[derive(Debug)]
pub struct NewRoom {
    pub slug: String,
    pub title: String,
    pub description: String,
    /// The owner of the new room
    pub user_id: String,
}

#[derive(Debug)]
pub struct UpdatedRoom {
    pub id: PrimaryKey,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug)]
pub struct NewRoomMember {
    pub user_id: PrimaryKey,
    pub room_id: PrimaryKey,
    pub owner: bool,
}

#[derive(Debug)]
pub struct NewRoomInvite {
    pub room_id: PrimaryKey,
    /// The inviter of the new room invite
    pub user_id: PrimaryKey,
}

#[derive(Debug)]
pub struct NewStreamKey {
    pub room_id: PrimaryKey,
    pub user_id: PrimaryKey,
    pub app: String,
}
