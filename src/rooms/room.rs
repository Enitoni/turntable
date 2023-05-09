use std::sync::Arc;

use crate::{
    audio::{new::Player, Queue, QueuePosition, Track},
    auth::User,
    db::{Database, Record},
    util::ApiError,
};

use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;

pub type RoomId = Thing;

#[derive(Debug, Deserialize, Serialize)]
pub struct RoomData {
    id: RoomId,
    name: String,
    owner: User,
}

#[derive(Debug, Clone)]
pub struct Room {
    pub id: RoomId,
    pub name: String,
    pub owner: User,

    pub player: Arc<Player>,
    pub queue: Arc<Queue>,
}

impl Room {
    fn from_data(raw: RoomData) -> Self {
        Self {
            id: raw.id,
            name: raw.name,
            owner: raw.owner,
            queue: Queue::new().into(),
            player: Player::default().into(),
        }
    }

    pub fn into_data(self) -> RoomData {
        RoomData {
            id: self.id,
            name: self.name,
            owner: self.owner,
        }
    }

    pub async fn create(db: &Database, user: &User, name: String) -> Result<Self, ApiError> {
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

        let raw: RoomData = Self::get(db, raw.id().to_string()).await?;

        Ok(Self::from_data(raw))
    }

    pub async fn all(db: &Database) -> Result<Vec<Self>, ApiError> {
        let raw_rooms: Vec<RoomData> = db
            .query("SELECT *, owner.* FROM room")
            .await?
            .take(0)
            .map_err(ApiError::Database)?;

        let results: Vec<_> = raw_rooms.into_iter().map(|r| Self::from_data(r)).collect();

        Ok(results)
    }

    pub async fn get(db: &Database, id: String) -> Result<RoomData, ApiError> {
        db.query("SELECT *, owner.* FROM type::thing($tb, $id)")
            .bind(("tb", "room"))
            .bind(("id", id))
            .await?
            .take::<Option<RoomData>>(0)?
            .ok_or(ApiError::NotFound("Room"))
    }

    fn update_player_sinks(&self) {
        let sinks: Vec<_> = self
            .queue
            .peek_ahead(3)
            .into_iter()
            .map(|t| t.sink)
            .collect();

        self.player.set_sinks(sinks);
    }

    pub fn next(&self) -> Track {
        let track = self.queue.next();
        self.update_player_sinks();

        track
    }

    pub fn add_track(&self, track: Track) {
        self.queue.add_track(track, QueuePosition::Add);
        self.update_player_sinks();
    }
}
