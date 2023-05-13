use std::sync::{Arc, Weak};

use dashmap::DashMap;

mod connection;
mod room;
mod router;

pub use room::*;
pub use router::router;

use crate::{
    audio::{AudioEvent, Player, PlayerId, Queue, QueuePosition, Track, WaveStream, SAMPLE_RATE},
    auth::{User, UserId},
    db::Database,
    events::{Event, Events, Handler},
    ingest::Input,
    server::ws::Recipients,
    store::{FromId, Store},
    util::ApiError,
    VinylEvent,
};

use self::connection::{Connection, ConnectionHandle, ConnectionHandleId};

#[derive(Debug)]
pub struct RoomManager {
    events: Events,

    me: Weak<RoomManager>,
    store: Weak<Store>,

    connections: DashMap<ConnectionHandleId, Connection>,
    queues: DashMap<RoomId, Queue>,
    rooms: DashMap<RoomId, Room>,
}

impl RoomManager {
    pub fn new(store: Weak<Store>, events: Events) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            events,
            store,
            me: me.clone(),
            rooms: Default::default(),
            connections: Default::default(),
            queues: Default::default(),
        })
    }

    fn store(&self) -> Arc<Store> {
        self.store.upgrade().expect("upgrade store in room manager")
    }

    fn set_up_room(&self, data: RoomData) -> RoomId {
        let store = self.store();

        let player = store.playback.create_player().expect("create player");
        let queue = Queue::new();

        let room = Room::new(data, queue.id, player);
        let id = room.id.clone();

        self.queues.insert(id.clone(), queue);
        self.rooms.insert(id.clone(), room);

        id
    }

    fn serialize_room(&self, id: &RoomId) -> SerializedRoom {
        let room = self.rooms.get(id).expect("get room").clone();
        let queue = self.queues.get(id).expect("get queue");
        let users = self.users_connected_to_room(id);

        SerializedRoom {
            id: room.id.id.to_string(),
            name: room.data.name,
            owner: room.data.owner,
            connections: users,
            current_track: queue.current_track(),
        }
    }

    pub async fn init(&self, db: &Database) -> Result<(), ApiError> {
        let rooms = RoomData::all(db).await?;

        for room in rooms {
            self.set_up_room(room);
        }

        Ok(())
    }

    pub async fn create_room(
        &self,
        db: &Database,
        user: &User,
        name: String,
    ) -> Result<SerializedRoom, ApiError> {
        let room = RoomData::create(db, user, name).await?;
        let id = self.set_up_room(room);

        Ok(self.serialize_room(&id))
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

    pub fn raw_rooms(&self) -> Vec<Room> {
        self.rooms.iter().map(|r| r.clone()).collect()
    }

    pub fn rooms(&self) -> Vec<SerializedRoom> {
        self.rooms
            .iter()
            .map(|r| self.serialize_room(&r.id))
            .collect()
    }

    /// Create a user's connection to a room, returning a streamable handle
    pub fn connect(&self, user: User, room: &RoomId) -> ConnectionHandle {
        let room = self.rooms.get(room).expect("room exists");

        let player: Arc<Player> = self.store().get(&room.player).expect("get player");
        let stream = WaveStream::new(player.consumer());

        let handle = ConnectionHandle::new(self.me.clone(), stream);

        let connection = Connection::new(handle.id, room.id.clone(), user.clone());
        self.connections.insert(handle.id, connection);

        let users_to_notify = self.user_ids_in_room(&room.id);

        self.events.emit(
            Event::UserEnteredRoom {
                room: room.id.clone(),
                user,
            },
            Recipients::Some(users_to_notify),
        );

        handle
    }

    // TODO: Fix this code when implementing proper queuing later
    pub fn add_input(&self, user: User, room: &RoomId, input: Input) {
        let queue = self.queues.get(room).expect("get queue");
        let room = self.rooms.get(room).expect("room exists");
        let users_to_notify = self.user_ids_in_room(&room.id);
        let player: Arc<Player> = self.store().get(&room.player).expect("get player");

        let sink = self
            .store
            .upgrade()
            .unwrap()
            .ingestion
            .add(input.loader().expect("loader"));

        let track = Track::new(input.duration(), input.to_string(), sink);

        self.events.emit(
            Event::QueueAdd {
                user: user.id,
                track: track.clone(),
            },
            Recipients::Some(users_to_notify),
        );

        queue.add_track(track, QueuePosition::Add);
        player.set_sinks(queue.peek_ahead(3).into_iter().map(|t| t.sink).collect());
    }

    pub fn handler(&self) -> RoomManagerHandler {
        RoomManagerHandler {
            manager: self.me.clone(),
        }
    }

    pub(self) fn notify_disconnect(&self, id: ConnectionHandleId) {
        let (_, connection) = self
            .connections
            .remove(&id)
            .expect("connection exists upon notify");

        let users_to_notify = self.user_ids_in_room(&connection.room);
        let user_not_in_room = users_to_notify.iter().all(|u| u != &connection.user.id);

        if user_not_in_room {
            self.events.emit(
                Event::UserLeftRoom {
                    room: connection.room,
                    user: connection.user.id,
                },
                Recipients::Some(users_to_notify),
            );
        }
    }
}

pub struct RoomManagerHandler {
    manager: Weak<RoomManager>,
}

impl RoomManagerHandler {
    fn handle_time(&self, player: PlayerId, offset: usize, _total_offset: usize) {
        let manager = self.manager();

        let room = manager
            .rooms
            .iter()
            .find(|r| r.player == player)
            .map(|x| x.clone())
            .expect("get room by player id");

        let users = manager.user_ids_in_room(&room.id);
        let time_in_seconds = offset as f32 / (SAMPLE_RATE * 2) as f32;

        manager.events.emit(
            Event::PlayerTime {
                room: room.id,
                seconds: time_in_seconds,
            },
            Recipients::Some(users),
        )
    }

    fn manager(&self) -> Arc<RoomManager> {
        self.manager
            .upgrade()
            .expect("upgrade room manager in handler")
    }
}

impl Handler<VinylEvent> for RoomManagerHandler {
    type Incoming = AudioEvent;

    fn handle(&self, incoming: Self::Incoming) {
        match incoming {
            AudioEvent::Time {
                player,
                total_offset,
                offset,
            } => self.handle_time(player, offset, total_offset),
            AudioEvent::Next { player } => todo!(),
        }
    }
}
