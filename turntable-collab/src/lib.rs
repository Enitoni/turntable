mod auth;
mod db;
mod events;
mod input;
mod queues;
mod rooms;
mod track;
mod util;

use auth::Auth;
use crossbeam::channel::unbounded;
use events::{CollabEvent, EventReceiver, EventSender};
use rooms::{RoomId, RoomManager};
use std::{sync::Arc, thread};

pub use auth::{AuthError, Credentials, NewPlainUser};
pub use db::*;
pub use input::*;
pub use queues::*;
pub use rooms::{Room, RoomConnection, RoomConnectionHandle, RoomError, RoomState};
pub use track::*;

use turntable_core::{ArcedStore, Config, Pipeline, PlayerId};
use turntable_impls::SymphoniaIngestion;

pub type CollabPipeline = Pipeline<SymphoniaIngestion>;
pub type CollabDatabase = PgDatabase;

/// The turntable collab system, facilitating room management, authentication, and more.
pub struct Collab {
    event_receiver: EventReceiver,

    pub auth: Auth<CollabDatabase>,
    pub rooms: RoomManager,
}

/// A type passed to various components of the collab system, to access state, emit events, and dispatch actions.
#[derive(Clone)]
pub struct CollabContext {
    event_sender: EventSender,

    pub pipeline: Arc<CollabPipeline>,
    pub database: Arc<CollabDatabase>,
    pub rooms: ArcedStore<RoomId, Room>,
}

impl Collab {
    pub async fn new(config: Config, database_url: &str) -> Self {
        let database = Arc::new(
            CollabDatabase::new(database_url)
                .await
                .expect("database is created"),
        );

        let pipeline = Arc::new(CollabPipeline::new(config));
        let (event_sender, event_receiver) = unbounded();

        let context = CollabContext {
            database: database.clone(),
            pipeline: pipeline.clone(),
            event_sender: event_sender.clone(),
            rooms: Default::default(),
        };

        let room_manager = RoomManager::new(&context);
        let auth = Auth::new(&database);

        let new = Self {
            auth,
            event_receiver,
            rooms: room_manager,
        };

        spawn_pipeline_event_conversion_thread(&context, &event_sender);

        new.init().await;
        new
    }

    /// Must be called after creation
    async fn init(&self) {
        self.rooms.restore().await.expect("rooms are restored");
    }

    /// Receive events from the collab.
    pub fn wait_for_event(&self) -> CollabEvent {
        self.event_receiver
            .recv()
            .expect("event is received without error")
    }
}

impl CollabContext {
    pub fn emit(&self, event: CollabEvent) {
        self.event_sender.send(event).expect("event is sent");
    }

    /// Gets a room by its player id if it exists and is active
    pub fn room_by_player_id(&self, player_id: PlayerId) -> Option<Arc<Room>> {
        self.rooms
            .iter()
            .find(|r| r.player().ok().filter(|p| p.id == player_id).is_some())
            .map(|r| r.clone())
    }
}

fn spawn_pipeline_event_conversion_thread(context: &CollabContext, sender: &EventSender) {
    let context = context.to_owned();
    let sender = sender.to_owned();

    let run = move || loop {
        let event = context.pipeline.wait_for_event();

        if let Some(converted_event) = CollabEvent::from_pipeline_event(&context, event) {
            sender.send(converted_event).expect("event is sent")
        }
    };

    thread::spawn(run);
}
