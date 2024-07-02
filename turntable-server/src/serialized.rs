//! All schemas that are exposed from endpoints are defined here
//! along with the From<T> impls

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Serialize;
use turntable_collab::{PrimaryKey, Room as CollabRoom, SessionData, UserData};

#[derive(Debug, Serialize, JsonSchema)]
pub struct User {
    id: PrimaryKey,
    username: String,
    display_name: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct LoginResult {
    token: String,
    user: User,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Room {
    id: PrimaryKey,
    title: String,
    description: Option<String>,
}

/// Helper trait to convert any type into a serialized version
pub trait ToSerialized<T>
where
    T: Serialize,
{
    fn to_serialized(&self) -> T;
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
        }
    }
}
