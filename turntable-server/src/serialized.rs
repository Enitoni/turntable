//! All schemas that are exposed from endpoints are defined here
//! along with the From<T> impls

use serde::Serialize;
use turntable_collab::{PrimaryKey, UserData};

#[derive(Debug, Serialize)]
pub struct User {
    id: PrimaryKey,
    username: String,
    display_name: String,
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
