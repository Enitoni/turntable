use std::{
    sync::{Arc, Weak},
    thread,
    time::{Duration, Instant},
};

use dashmap::DashMap;

mod connection;
mod room;
mod router;

use log::warn;
pub use room::*;
pub use router::router;

use crate::{
    audio::{
        Track, WaveStream, PRELOAD_AMOUNT, SAMPLES_PER_SEC, SAMPLE_RATE, STREAM_CHUNK_DURATION,
    },
    auth::{User, UserId},
    db::Database,
    events::{Event, Events},
    ingest::{run_ingestion, Ingestion, Input},
    server::ws::Recipients,
    util::ApiError,
};

use self::connection::{Connection, ConnectionHandle, ConnectionHandleId};

#[derive(Debug)]
pub struct RoomManager {
    me: Weak<RoomManager>,

    events: Events,
    ingestion: Arc<Ingestion>,

    connections: DashMap<ConnectionHandleId, connection::Connection>,
    rooms: DashMap<RoomId, Room>,
}

impl RoomManager {
    pub fn new(events: Events) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            events,
            me: me.clone(),
            rooms: Default::default(),
            connections: Default::default(),
            ingestion: Ingestion::new().into(),
        })
    }

    pub async fn init(&self, db: &Database) -> Result<(), ApiError> {
        let rooms = Room::all(db).await?;

        for room in rooms {
            self.rooms.insert(room.id.clone(), room);
        }

        Ok(())
    }

    pub async fn create_room(
        &self,
        db: &Database,
        user: &User,
        name: String,
    ) -> Result<Room, ApiError> {
        let room = Room::create(db, user, name).await?;
        self.rooms.insert(room.id.clone(), room.clone());

        Ok(room)
    }

    fn users_connected_to_room(&self, id: &RoomId) -> Vec<User> {
        self.connections
            .iter()
            .filter(|c| c.room == id.clone())
            .map(|c| c.user.clone())
            .collect()
    }

    fn user_ids_in_room(&self, id: &RoomId) -> Vec<UserId> {
        self.users_connected_to_room(id)
            .into_iter()
            .map(|u| u.id)
            .collect()
    }

    pub fn rooms(&self) -> Vec<Room> {
        self.rooms.iter().map(|r| r.value().clone()).collect()
    }

    /// Create a user's connection to a room, returning a streamable handle
    pub fn connect(&self, user: User, room: &RoomId) -> ConnectionHandle {
        let room = self.rooms.get(room).expect("room exists");
        let stream = WaveStream::new(room.player.consumer());

        let handle = ConnectionHandle::new(self.me.clone(), stream);
        let connection = Connection::new(handle.id, room.id.clone(), user);

        let users_to_notify = self.user_ids_in_room(&room.id);

        self.events.emit(
            Event::UserEnteredRoom {
                room: connection.room.clone(),
                user: connection.user.clone(),
            },
            Recipients::Some(users_to_notify),
        );

        self.connections.insert(handle.id, connection);

        handle
    }

    // TODO: Fix this code when implementing proper queuing later
    pub fn add_input(&self, user: User, room: &RoomId, input: Input) {
        let room = self.rooms.get(room).expect("room exists");
        let users_to_notify = self.user_ids_in_room(&room.id);

        let sink = self.ingestion.add(input.loader().expect("loader"));
        let track = Track::new(input.duration(), input.to_string(), sink);

        self.events.emit(
            Event::QueueAdd {
                user: user.id,
                track: track.clone(),
            },
            Recipients::Some(users_to_notify),
        );

        room.add_track(track);
    }

    pub(self) fn notify_disconnect(&self, id: ConnectionHandleId) {
        let (_, connection) = self
            .connections
            .remove(&id)
            .expect("connection exists upon notify");

        let users_to_notify = self.user_ids_in_room(&connection.room);

        self.events.emit(
            Event::UserLeftRoom {
                room: connection.room,
                user: connection.user.id,
            },
            Recipients::Some(users_to_notify),
        );
    }
}

fn process_load_requests(manager: Arc<RoomManager>) {
    let ingestion = manager.ingestion.clone();

    let sinks_to_load: Vec<_> = manager
        .rooms
        .iter()
        .filter_map(|r| r.player.preload())
        .collect();

    for sink in sinks_to_load {
        ingestion.request(sink, PRELOAD_AMOUNT);
    }
}

fn process_rooms(manager: Arc<RoomManager>) {
    let events = manager.events.clone();

    let rooms: Vec<_> = manager
        .rooms
        .iter()
        // TODO: Fix this .filter(|r| r.is_active())
        .map(|r| r.clone())
        .collect();

    for room in rooms {
        let processed = room.player.process();

        let seconds = processed.new_sink_offset as f32 / (SAMPLE_RATE * 2) as f32;
        let users_to_notify = manager.user_ids_in_room(&room.id);

        events.emit(
            Event::PlayerTime {
                room: room.id.clone(),
                seconds,
            },
            Recipients::Some(users_to_notify.clone()),
        );

        for _ in 0..processed.consumed_sinks {
            let track = room.next();

            events.emit(
                Event::TrackUpdate {
                    room: room.id.clone(),
                    track,
                },
                Recipients::Some(users_to_notify.clone()),
            )
        }
    }
}

fn spawn_room_loading_thread(manager: Arc<RoomManager>) {
    let run = move || loop {
        process_load_requests(manager.clone());
        thread::sleep(Duration::from_millis(100));
    };

    thread::Builder::new()
        .name("room_loading".to_string())
        .spawn(run)
        .unwrap();
}

fn spawn_room_processing_thread(manager: Arc<RoomManager>) {
    let run = move || loop {
        let now = Instant::now();
        process_rooms(manager.clone());
        wait_for_next(now);
    };

    thread::Builder::new()
        .name("room_processing".to_string())
        .spawn(run)
        .unwrap();
}

pub fn run_room_manager(manager: Arc<RoomManager>) {
    spawn_room_loading_thread(manager.clone());
    spawn_room_processing_thread(manager.clone());
    run_ingestion(manager.ingestion.clone());
}

fn wait_for_next(now: Instant) {
    let elapsed = now.elapsed();
    let elapsed_micros = elapsed.as_micros();
    let elapsed_millis = elapsed_micros / 1000;

    let duration_micros = STREAM_CHUNK_DURATION.as_micros();

    if elapsed_millis > SAMPLES_PER_SEC as u128 / 10000 {
        warn!(
            "Stream took too long ({}ms) to process samples!",
            elapsed_millis
        )
    }

    let corrected = duration_micros
        .checked_sub(elapsed_micros)
        .unwrap_or_default();

    spin_sleep::sleep(Duration::from_micros(corrected as u64));
}
