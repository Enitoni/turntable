//! All schemas that are exposed from endpoints are defined here
//! along with the From<T> impls

use std::sync::Arc;

use serde::Serialize;
use turntable_collab::{
    Room as CollabRoom, RoomConnection as CollabRoomConnection, RoomMemberData, SessionData,
    StreamKeyData, UserData,
};
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct User {
    id: i32,
    username: String,
    display_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResult {
    token: String,
    user: User,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Room {
    id: i32,
    title: String,
    description: Option<String>,
    members: Vec<RoomMember>,
    connections: Vec<RoomConnection>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RoomMember {
    id: i32,
    owner: bool,
    user: User,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RoomConnection {
    user_id: i32,
    source: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StreamKey {
    id: i32,
    token: String,
    source: String,
    room_id: i32,
    user_id: i32,
}

/// Helper trait to convert any type into a serialized version
pub trait ToSerialized<T>
where
    T: Serialize,
{
    fn to_serialized(&self) -> T;
}

impl<I, O> ToSerialized<Vec<O>> for Vec<I>
where
    I: ToSerialized<O>,
    O: Serialize,
{
    fn to_serialized(&self) -> Vec<O> {
        self.iter().map(|x| x.to_serialized()).collect()
    }
}

impl ToSerialized<User> for UserData {
    fn to_serialized(&self) -> User {
        User {
            id: self.id,
            username: self.username.clone(),
            display_name: self.display_name.clone(),
        }
    }
}

impl ToSerialized<LoginResult> for SessionData {
    fn to_serialized(&self) -> LoginResult {
        LoginResult {
            token: self.token.clone(),
            user: self.user.to_serialized(),
        }
    }
}

impl ToSerialized<Room> for Arc<CollabRoom> {
    fn to_serialized(&self) -> Room {
        let data = self.data();

        Room {
            id: data.id,
            title: data.title,
            description: data.description,
            members: data.members.to_serialized(),
            connections: self.current_connections().to_serialized(),
        }
    }
}

impl ToSerialized<RoomMember> for RoomMemberData {
    fn to_serialized(&self) -> RoomMember {
        RoomMember {
            id: self.id,
            owner: self.owner,
            user: self.user.to_serialized(),
        }
    }
}

impl ToSerialized<RoomConnection> for CollabRoomConnection {
    fn to_serialized(&self) -> RoomConnection {
        RoomConnection {
            user_id: self.user_id,
            source: self.source.clone(),
        }
    }
}

impl ToSerialized<StreamKey> for StreamKeyData {
    fn to_serialized(&self) -> StreamKey {
        StreamKey {
            id: self.id,
            token: self.token.clone(),
            source: self.source.clone(),
            room_id: self.room_id,
            user_id: self.user_id,
        }
    }
}
