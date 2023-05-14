use crate::{
    auth::User,
    db::{Database, Record},
    queue::QueueItem,
    track::Track,
    util::ApiError,
};

use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;

pub type RoomId = Thing;

#[derive(Debug, Clone, Deserialize)]
pub struct RoomData {
    pub id: RoomId,
    pub name: String,
    pub owner: User,
}

impl RoomData {
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

        Self::get(db, raw.id().to_string()).await
    }

    pub async fn all(db: &Database) -> Result<Vec<Self>, ApiError> {
        let raw_rooms: Vec<RoomData> = db
            .query("SELECT *, owner.* FROM room")
            .await?
            .take(0)
            .map_err(ApiError::Database)?;

        Ok(raw_rooms)
    }

    pub async fn get(db: &Database, id: String) -> Result<Self, ApiError> {
        db.query("SELECT *, owner.* FROM type::thing($tb, $id)")
            .bind(("tb", "room"))
            .bind(("id", id))
            .await?
            .take::<Option<RoomData>>(0)?
            .ok_or(ApiError::NotFound("Room"))
    }
}

#[derive(Debug, Clone)]
pub struct Room {
    pub id: RoomId,
    pub data: RoomData,
}

impl Room {
    pub fn new(data: RoomData) -> Self {
        Self {
            id: data.id.clone(),
            data,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializedRoom {
    pub id: String,
    pub name: String,
    pub owner: User,
    pub connections: Vec<User>,
    pub current_queue_item: Option<QueueItem>,
}
