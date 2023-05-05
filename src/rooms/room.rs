use std::{
    convert::Infallible,
    io::Read,
    pin::Pin,
    sync::{Arc, Weak},
    task::{Context, Poll},
};

use crate::{
    audio::WaveStream,
    auth::User,
    db::{Database, Record},
    util::ApiError,
};
use crossbeam::atomic::AtomicCell;
use futures_util::Stream;
use parking_lot::{Mutex, RwLock};
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

#[derive(Debug, Clone, Deserialize)]
pub struct Room {
    #[serde(skip)]
    manager: Weak<RoomManager>,

    pub id: RoomId,
    pub name: String,
    pub owner: User,

    #[serde(skip_deserializing)]
    connections: Arc<RwLock<Vec<(ConnectionId, User)>>>,
}

static ID_COUNTER: AtomicCell<u64> = AtomicCell::new(1);

/// Describes a connection to the stream of this room
#[derive(Debug, Serialize)]
pub struct Connection {
    #[serde(skip)]
    id: ConnectionId,

    #[serde(skip)]
    manager: Weak<RoomManager>,

    #[serde(skip)]
    room_id: RoomId,

    #[serde(skip)]
    stream: WaveStream,

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
            name: String,
            owner: Thing,
        }

        let raw: Record = db
            .create("room")
            .content(NewRoom {
                owner: user.id.clone(),
                name,
            })
            .await
            .map_err(ApiError::from_db)?;

        let raw: RawRoom = Self::get(db, raw.id().to_string()).await?;

        Ok(Self::from_raw(raw, manager))
    }

    pub async fn all(db: &Database, manager: Weak<RoomManager>) -> Result<Vec<Self>, ApiError> {
        let raw_rooms: Vec<RawRoom> = db
            .query("SELECT *, owner.* FROM room")
            .await?
            .take(0)
            .map_err(ApiError::Database)?;

        let results: Vec<_> = raw_rooms
            .into_iter()
            .map(|r| Self::from_raw(r, manager.clone()))
            .collect();

        Ok(results)
    }

    pub async fn get(db: &Database, id: String) -> Result<RawRoom, ApiError> {
        db.query("SELECT *, owner.* FROM type::thing($tb, $id)")
            .bind(("tb", "room"))
            .bind(("id", id))
            .await?
            .take::<Option<RawRoom>>(0)?
            .ok_or(ApiError::NotFound("Room"))
    }

    pub fn connect(&self, stream: WaveStream, user: User) -> Connection {
        let connection = Connection {
            id: ID_COUNTER.fetch_add(1),
            manager: self.manager.clone(),
            room_id: self.id.clone(),
            stream,
            user,
        };

        self.connections
            .write()
            .push((connection.id, connection.user.clone()));

        connection
    }

    pub fn disconnect(&self, id: ConnectionId) {
        self.connections.write().retain(|(c, _)| *c != id);
    }

    pub fn into_raw(self) -> RawRoom {
        RawRoom {
            id: self.id,
            name: self.name,
            owner: self.owner,
            connections: self
                .connections
                .read()
                .iter()
                .map(|(_, u)| u.clone())
                .collect(),
        }
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

impl Stream for Connection {
    type Item = Result<Vec<u8>, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut buf = vec![0; 2048];

        self.stream.read(&mut buf).ok();

        Poll::Ready(Some(Ok(buf)))
    }
}