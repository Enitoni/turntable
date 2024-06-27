mod auth;
mod db;
mod input;
mod queues;
mod rooms;
mod track;
mod util;

use std::sync::Arc;

use auth::Auth;
pub use db::*;
pub use input::*;
pub use queues::*;
use rooms::{Room, RoomId, RoomManager};
pub use track::*;

use turntable_core::{ArcedStore, Ingestion, Pipeline};

/// The turntable collab system, facilitating room management, authentication, and more.
pub struct Collab<I, Db> {
    pipeline: Arc<Pipeline<I>>,
    database: Arc<Db>,

    pub auth: Auth<Db>,
    pub rooms: RoomManager<I, Db>,
}

/// A type passed to various components of the collab system, to access state, emit events, and dispatch actions.
pub struct CollabContext<I, Db> {
    pub pipeline: Arc<Pipeline<I>>,
    pub database: Arc<Db>,

    pub rooms: ArcedStore<RoomId, Room<I, Db>>,
}

impl<I, Db> Collab<I, Db>
where
    I: Ingestion,
    Db: Database,
{
    pub fn new(pipeline: Pipeline<I>, database: Db) -> Self {
        let database = Arc::new(database);
        let pipeline = Arc::new(pipeline);

        let context = CollabContext {
            database: database.clone(),
            pipeline: pipeline.clone(),

            rooms: Default::default(),
        };

        let room_manager = RoomManager::new(&context);
        let auth = Auth::new(&database);

        Self {
            auth,
            pipeline,
            database,
            rooms: room_manager,
        }
    }

    /// Must be called after creation
    pub async fn init(&self) {
        self.rooms.restore().await.expect("rooms are restored");
    }
}

impl<I, Db> Clone for CollabContext<I, Db>
where
    I: Ingestion,
    Db: Database,
{
    fn clone(&self) -> Self {
        Self {
            database: self.database.clone(),
            pipeline: self.pipeline.clone(),
            rooms: self.rooms.clone(),
        }
    }
}
