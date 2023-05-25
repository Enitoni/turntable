use std::sync::{Arc, Weak};

use dashmap::DashMap;

mod connection;
mod room;
mod router;

pub use room::*;
pub use router::router;

use crate::{
    audio::{AudioEvent, PlayerId, WaveStream, SAMPLE_RATE},
    auth::{User, UserId},
    db::Database,
    events::{Event, Events, Filter, Handler},
    ingest::Input,
    queue::{QueueEvent, QueueId, QueueItem, SerializedQueue, SubQueueId},
    server::ws::Recipients,
    store::Store,
    track::InternalTrack,
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

    queues: DashMap<RoomId, QueueId>,
    sub_queues: DashMap<(RoomId, UserId), SubQueueId>,

    players: DashMap<RoomId, PlayerId>,
    rooms: DashMap<RoomId, Room>,
}

impl RoomManager {
    pub fn new(store: Weak<Store>, events: Events) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            events,
            store,
            me: me.clone(),
            rooms: Default::default(),
            players: Default::default(),
            connections: Default::default(),
            sub_queues: Default::default(),
            queues: Default::default(),
        })
    }

    fn store(&self) -> Arc<Store> {
        self.store.upgrade().expect("upgrade store in room manager")
    }

    fn set_up_room(&self, data: RoomData) -> RoomId {
        let store = self.store();

        let player = store.playback.create_player().expect("create player");
        let queue = store.queue_store.create_queue(player);

        let room = Room::new(data);
        let id = room.id.clone();

        self.players.insert(id.clone(), player);
        self.queues.insert(id.clone(), queue);
        self.rooms.insert(id.clone(), room);

        id
    }

    fn serialize_room(&self, id: &RoomId) -> SerializedRoom {
        let store = self.store();

        let room = self.rooms.get(id).expect("get room").clone();
        let queue = self.queues.get(id).expect("get queue");
        let users = self.users_connected_to_room(id);

        let current_queue_item = store.queue_store.current_item(*queue);

        SerializedRoom {
            id: room.id.id.to_string(),
            name: room.data.name,
            owner: room.data.owner,
            connections: users,
            current_queue_item,
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

    fn ensure_sub_queue(&self, user: User, room: &RoomId) {
        let queue = self.queues.get(room).expect("queue exists");

        self.sub_queues
            .entry((room.clone(), user.id.clone()))
            .or_insert_with(|| self.store().queue_store.create_sub_queue(*queue, user));
    }

    pub fn queue(&self, room: &RoomId) -> SerializedQueue {
        let queue = self.queues.get(room).expect("queue exists");
        self.store().queue_store.serialized(*queue)
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
    pub fn connect(&self, user: User, room_id: &RoomId) -> ConnectionHandle {
        let store = self.store();
        let room = self.rooms.get(room_id).expect("room exists");

        let player = self
            .players
            .get(room_id)
            .expect("player exists")
            .upgrade(&store);

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
        self.ensure_sub_queue(user.clone(), room);

        let sub_queue = self
            .sub_queues
            .get(&(room.clone(), user.id))
            .expect("get queue");

        let _room = self.rooms.get(room).expect("room exists");
        //let users_to_notify = self.user_ids_in_room(&room.id);

        // TODO: Make this part of the track store
        let track = InternalTrack::new(input);
        self.store().queue_store.add(*sub_queue, vec![track.into()]);
    }

    pub fn handler(&self) -> RoomManagerHandler {
        RoomManagerHandler {
            manager: self.me.clone(),
        }
    }

    pub(self) fn notify_disconnect(&self, id: ConnectionHandleId) {
        let connection = self
            .connections
            .get(&id)
            .expect("connection exists")
            .clone();

        let users_to_notify = self.user_ids_in_room(&connection.room);

        let (_, connection) = self
            .connections
            .remove(&id)
            .expect("connection exists upon notify");

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

#[derive(Debug)]
pub enum RoomManagerEvent {
    Queue(QueueEvent),
    Audio(AudioEvent),
}

impl RoomManagerHandler {
    fn handle_time(&self, player: PlayerId, offset: usize, _total_offset: usize) {
        let manager = self.manager();

        let room = manager
            .players
            .iter()
            .find(|r| r.value() == &player)
            .map(|x| x.key().clone())
            .expect("get room by player id");

        let users = manager.user_ids_in_room(&room);
        let time_in_seconds = offset as f32 / (SAMPLE_RATE * 2) as f32;

        manager.events.emit(
            Event::PlayerTime {
                room,
                seconds: time_in_seconds,
            },
            Recipients::Some(users),
        )
    }

    fn handle_audio_event(&self, event: AudioEvent) {
        if let AudioEvent::Time {
            player,
            total_offset,
            offset,
        } = event
        {
            self.handle_time(player, offset, total_offset)
        }
    }

    fn handle_queue_event(&self, event: QueueEvent) {
        match event {
            QueueEvent::Update { queue, new_items } => self.handle_update(queue, new_items),
            QueueEvent::Advance { queue, item } => self.handle_next(queue, item),
        }
    }

    fn handle_update(&self, queue: QueueId, new_items: Vec<QueueItem>) {
        let manager = self.manager();

        let room = manager
            .queues
            .iter()
            .find(|r| r.value() == &queue)
            .map(|x| x.key().clone())
            .expect("get room by queue id");

        let users_to_notify = manager.user_ids_in_room(&room);

        manager.events.emit(
            Event::QueueUpdate {
                room,
                items: new_items,
            },
            Recipients::Some(users_to_notify),
        )
    }

    fn handle_next(&self, queue: QueueId, item: QueueItem) {
        let manager = self.manager();

        let room = manager
            .queues
            .iter()
            .find(|r| r.value() == &queue)
            .map(|x| x.key().clone())
            .expect("get room by queue id");

        let users_to_notify = manager.user_ids_in_room(&room);

        manager.events.emit(
            Event::QueueAdvance { room, item },
            Recipients::Some(users_to_notify),
        )
    }

    fn manager(&self) -> Arc<RoomManager> {
        self.manager
            .upgrade()
            .expect("upgrade room manager in handler")
    }
}

impl Handler<VinylEvent> for RoomManagerHandler {
    type Incoming = RoomManagerEvent;

    fn handle(&self, incoming: Self::Incoming) {
        match incoming {
            RoomManagerEvent::Audio(x) => self.handle_audio_event(x),
            RoomManagerEvent::Queue(x) => self.handle_queue_event(x),
        }
    }
}

impl Filter<VinylEvent> for RoomManagerEvent {
    fn filter(event: VinylEvent) -> Option<Self> {
        match event {
            VinylEvent::Audio(x) => Some(Self::Audio(x)),
            VinylEvent::Queue(x) => Some(Self::Queue(x)),
            _ => None,
        }
    }
}
