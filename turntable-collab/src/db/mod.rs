use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;

mod data;
pub use data::*;

mod pg;
pub use pg::*;

pub type Result<T> = std::result::Result<T, DatabaseError>;
pub type BoxedDatabase = Box<dyn Database>;

#[derive(Debug, Error)]
pub enum DatabaseError {
    /// An unknown or internal error happened with the database
    #[error(transparent)]
    Internal(Box<dyn std::error::Error>),
    /// A resource already exists
    #[error("{resource} with {field} of value {value} already exists")]
    Conflict {
        /// The resource in question
        resource: &'static str,
        /// The field that is conflicting
        field: &'static str,
        /// The conflicting value
        value: String,
    },
    /// A resource in the database doesn't exist
    #[error("{resource}:{identifier} doesn't exist")]
    NotFound {
        resource: &'static str,
        identifier: &'static str,
    },
}

/// Helper trait to reduce boilerplate
pub trait IntoDatabaseError {
    fn not_found_or(self, resource: &'static str, identifier: &'static str) -> DatabaseError;
    fn any(self) -> DatabaseError;
}

/// Helper trait to reduce boilerplate
pub trait DatabaseResult {
    /// Turns the Result into a conflict error if it's Ok()
    fn conflict_or_ok(self, resource: &'static str, field: &'static str, value: &str)
        -> Result<()>;
}

impl<T> DatabaseResult for Result<T> {
    fn conflict_or_ok(
        self,
        resource: &'static str,
        field: &'static str,
        value: &str,
    ) -> Result<()> {
        match self {
            Ok(_) => Err(DatabaseError::Conflict {
                resource,
                field,
                value: value.to_string(),
            }),
            Err(e) => match e {
                DatabaseError::NotFound {
                    resource: _,
                    identifier: _,
                } => Ok(()),
                e => Err(e),
            },
        }
    }
}

/// Represents a type that can fetch turntable data from a database
#[async_trait]
pub trait Database {
    async fn check_for_superuser(&self) -> Result<bool>;
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
    async fn create_room_member(&self, new_member: NewRoomMember) -> Result<RoomMemberData>;
    async fn update_room(&self, updated_room: UpdatedRoom) -> Result<RoomData>;
    async fn delete_room(&self, room_id: PrimaryKey) -> Result<()>;
    async fn delete_room_member(&self, room_id: PrimaryKey, user_id: PrimaryKey) -> Result<()>;
    async fn create_room_invite(&self, new_room_invite: NewRoomInvite) -> Result<RoomInviteData>;
    async fn delete_room_invite(&self, invite_id: PrimaryKey) -> Result<()>;

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
    pub superuser: bool,
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
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct NewRoom {
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    /// The owner of the new room
    pub user_id: PrimaryKey,
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
    pub token: String,
    pub room_id: PrimaryKey,
    /// The inviter of the new room invite
    pub user_id: PrimaryKey,
}

#[derive(Debug)]
pub struct NewStreamKey {
    pub token: String,
    pub room_id: PrimaryKey,
    pub user_id: PrimaryKey,
    pub source: String,
}
