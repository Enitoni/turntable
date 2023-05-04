use std::sync::{Arc, Weak};

use crate::{
    audio::WaveStream,
    auth::User,
    db::{Database, Record},
    util::ApiError,
};
use crossbeam::atomic::AtomicCell;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;

use super::RoomManager;

pub type ConnectionId = u64;
pub type RoomId = Thing;

#[derive(Debug, Deserialize, Serialize)]
pub struct RawRoom {
    id: RoomId,
    name: String,
    owner: User,

    #[serde(skip_deserializing)]
    connections: Vec<User>,
}

#[derive(Debug, Deserialize)]
pub struct Room {
    #[serde(skip)]
    manager: Weak<RoomManager>,

    pub id: RoomId,
    pub name: String,
    pub owner: User,

    #[serde(skip_deserializing)]
    connections: Mutex<Vec<Connection>>,
}

static ID_COUNTER: AtomicCell<u64> = AtomicCell::new(1);

/// Describes a connection to the stream of this room
#[derive(Debug, Clone, Serialize)]
pub struct Connection {
    #[serde(skip)]
    id: ConnectionId,

    #[serde(skip)]
    manager: Weak<RoomManager>,

    #[serde(skip)]
    room_id: RoomId,

    #[serde(skip)]
    stream: Arc<WaveStream>,

    user: User,
}

impl Room {
    fn from_raw(raw: RawRoom, manager: Weak<RoomManager>) -> Self {
        Self {
            manager,
            id: raw.id,
            name: raw.name,
            owner: raw.owner,
            connections: Default::default(),
        }
    }

    pub async fn create(
        db: &Database,
        manager: Weak<RoomManager>,
        user: &User,
        name: String,
    ) -> Result<Self, ApiError> {
        #[derive(Serialize)]
        struct NewRoom {
            id: String,
            name: String,
            owner: Thing,
        }

        let raw: RawRoom = db
            .create("room")
            .content(NewRoom {
                owner: user.id.clone(),
                id: name.clone(),
                name,
            })
            .await
            .map_err(ApiError::from_db)?;

        Ok(Self::from_raw(raw, manager))
    }

    pub async fn all(db: &Database, manager: Weak<RoomManager>) -> Result<Vec<Self>, ApiError> {
        let raw_rooms: Vec<RawRoom> = db
            .query("SELECT * FROM room")
            .await?
            .take(0)
            .map_err(ApiError::Database)?;

        let results: Vec<_> = raw_rooms
            .into_iter()
            .map(|r| Self::from_raw(r, manager.clone()))
            .collect();

        Ok(results)
    }

    pub fn connect(&self, stream: WaveStream, user: User) -> Connection {
        let connection = Connection {
            id: ID_COUNTER.fetch_add(1),
            manager: self.manager.clone(),
            room_id: self.id.clone(),
            stream: stream.into(),
            user,
        };

        self.connections.lock().push(connection.clone());

        connection
    }

    pub fn disconnect(&self, id: ConnectionId) {
        self.connections.lock().retain(|c| c.id != id)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.manager
            .upgrade()
            .expect("upgrade weak")
            .notify_disconnect(&self.room_id, self.id)
    }
}
