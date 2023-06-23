use std::sync::{Arc, Weak};

use dashmap::DashMap;

use crate::{
    audio::{Input, PlayerId, WaveStream},
    auth::{User, UserId},
    db::Database,
    queue::{QueueId, SubQueueId},
    store::{FromId, Store},
    track::InternalTrack,
    util::ApiError,
    EventEmitter,
};

use super::{
    connection::{Connection, ConnectionHandle, ConnectionHandleId},
    RoomData, RoomEvent, RoomId, SerializedRoom,
};

#[derive(Debug)]
pub struct RoomStore {
    store: Weak<Store>,
    emitter: EventEmitter,

    pub(super) rooms: DashMap<RoomId, RoomData>,
    pub(super) queues: DashMap<RoomId, QueueId>,
    pub(super) players: DashMap<RoomId, PlayerId>,
    pub(super) sub_queues: DashMap<(RoomId, UserId), SubQueueId>,
    pub(super) connections: DashMap<ConnectionHandleId, Connection>,
}

impl RoomStore {
    pub fn new(store: Weak<Store>, emitter: EventEmitter) -> Self {
        Self {
            store,
            emitter,
            rooms: Default::default(),
            queues: Default::default(),
            players: Default::default(),
            sub_queues: Default::default(),
            connections: Default::default(),
        }
    }

    pub async fn init(&self, db: &Database) -> Result<(), ApiError> {
        RoomData::all(db).await?.into_iter().for_each(|r| {
            self.set_up_room(r);
        });

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
        let handle = ConnectionHandle::new(self.store.clone(), stream);

        let connection = Connection::new(handle.id, room.id.clone(), user.clone());

        self.connections.insert(handle.id, connection);

        self.emitter.dispatch(RoomEvent::UserEnteredRoom {
            room: room.id.clone(),
            user,
        });

        handle
    }

    pub(super) fn notify_disconnect(&self, id: ConnectionHandleId) {
        let (_, connection) = self
            .connections
            .remove(&id)
            .expect("connection exists upon notify");

        let user_not_in_room = self
            .users_in_room(&connection.room)
            .into_iter()
            .all(|u| u.id != connection.user.id);

        if user_not_in_room {
            self.emitter.dispatch(RoomEvent::UserLeftRoom {
                room: connection.room,
                user: connection.user.id,
            });
        }
    }

    // TODO: Fix this code when implementing proper queuing later
    pub fn add_input(&self, user: User, room: &RoomId, input: Input) {
        self.ensure_sub_queue(user.clone(), room);

        let sub_queue = self
            .sub_queues
            .get(&(room.clone(), user.id))
            .expect("get queue");

        // TODO: Make this part of the track store
        let track = InternalTrack::new(input);
        self.store().queue_store.add(*sub_queue, vec![track.into()]);
    }

    fn store(&self) -> Arc<Store> {
        self.store.upgrade().expect("upgrade store in room manager")
    }

    fn set_up_room(&self, room: RoomData) -> RoomId {
        let store = self.store();

        let player = store.playback.create_player().expect("create player");
        let queue = store.queue_store.create_queue(player);

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
        let users = self.users_in_room(id);

        let current_queue_item = store.queue_store.current_item(*queue);

        SerializedRoom {
            id: room.id.id.to_string(),
            name: room.name,
            owner: room.owner,
            connections: users,
            current_queue_item,
        }
    }

    fn users_in_room(&self, id: &RoomId) -> Vec<User> {
        self.connections
            .iter()
            .filter(|c| c.room == id.clone())
            .map(|c| c.user.clone())
            .collect()
    }

    fn ensure_sub_queue(&self, user: User, room: &RoomId) {
        let queue = self.queues.get(room).expect("queue exists");

        self.sub_queues
            .entry((room.clone(), user.id.clone()))
            .or_insert_with(|| self.store().queue_store.create_sub_queue(*queue, user));
    }
}

impl FromId<PlayerId> for RoomId {
    type Output = RoomId;

    fn from_id(store: &Store, id: &PlayerId) -> Option<Self::Output>
    where
        Self: Sized,
    {
        store
            .room_store
            .players
            .iter()
            .find_map(|x| (x.value() == id).then(|| x.key().clone()))
    }
}
