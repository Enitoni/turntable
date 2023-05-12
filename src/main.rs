use std::{sync::Arc, thread};

use colored::Colorize;
use db::Database;
use events::{Bus, Channel, Emitter, Events};
use ingest::IngestionEvent;
use log::{error, info};
use server::ws::WebSocketManager;
use thiserror::Error;
use tokio::runtime::{self, Runtime};

use crate::{
    logging::{EventLogger, LogColor},
    rooms::RoomManager,
};

mod audio;
mod auth;
mod db;
mod events;
mod http;
mod ingest;
mod logging;
mod rooms;
mod server;
mod util;

pub struct Vinyl {
    db: Arc<Database>,
    event_bus: Arc<EventBus>,
    websockets: Arc<WebSocketManager>,
    rooms: Arc<RoomManager>,
    events: Events,
    runtime: Runtime,
}

#[derive(Debug, Clone)]
pub enum VinylEvent {
    Ingestion(IngestionEvent),
}

pub type EventEmitter = Emitter<Channel<VinylEvent>, VinylEvent>;
pub type EventBus = Bus<Channel<VinylEvent>, VinylEvent>;

#[derive(Clone)]
pub struct VinylContext {
    pub events: Events,
    pub db: Arc<Database>,
    pub rooms: Arc<RoomManager>,
    pub websockets: Arc<WebSocketManager>,
}

#[derive(Debug, Error)]
enum VinylError {
    #[error("Could not initialize database: {0}")]
    Database(#[from] surrealdb::Error),

    #[error("Fatal error: {0}")]
    Fatal(String),
}

impl Vinyl {
    fn new() -> Result<Self, VinylError> {
        info!("Building async runtime...");
        let main_runtime = runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("vinyl-async")
            .build()
            .map_err(|e| VinylError::Fatal(e.to_string()))?;

        info!("Connecting to database...");

        let channel = Channel::new();
        let event_bus = EventBus::new(channel);

        event_bus.register(EventLogger);

        let events = Events::default();
        let rooms = RoomManager::new(events.clone(), event_bus.emitter());

        let database = main_runtime.block_on(db::connect())?;

        main_runtime
            .block_on(rooms.init(&database))
            .map_err(|e| VinylError::Fatal(e.to_string()))?;

        Ok(Self {
            rooms,
            events,
            event_bus,
            db: database.into(),
            websockets: WebSocketManager::new(),
            runtime: main_runtime,
        })
    }

    fn run(&self) {
        rooms::run_room_manager(self.rooms.clone());

        self.runtime.block_on(async move {
            tokio::spawn(events::check_events(self.context()));
            server::run_server(self.context()).await
        });

        let event_bus = self.event_bus.clone();
        thread::spawn(move || loop {
            event_bus.tick()
        });
    }

    fn context(&self) -> VinylContext {
        VinylContext {
            db: self.db.clone(),
            rooms: self.rooms.clone(),
            events: self.events.clone(),
            websockets: self.websockets.clone(),
        }
    }
}

impl VinylError {
    fn hint(&self) -> String {
        match self {
            VinylError::Database(_) => "This is a database error. Make sure the SurrealDB instance is properly installed and running, then try again.".to_string(),
            VinylError::Fatal(_) => "This error is fatal, and should not happen.".to_string(),
        }
    }
}

fn main() {
    logging::init_logger();

    match Vinyl::new() {
        Ok(vinyl) => {
            info!("Initialized successfully.");
            vinyl.run();
        }
        Err(error) => {
            error!("{} Read the error below to troubleshoot the issue. If you think this might be a bug, please report it by making a GitHub issue.", "Vinyl failed to start!".bold().color(LogColor::Red));
            error!("{}", error);
            error!(
                "{}",
                format!("Hint: {}", error.hint())
                    .color(LogColor::Dimmed)
                    .italic()
            );
        }
    }
}
