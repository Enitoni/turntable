use std::{
    convert::Infallible,
    pin::Pin,
    sync::{Arc, Weak},
    task::{Context, Poll, Waker},
};

use axum::{
    extract::State,
    response::{
        sse::{Event, KeepAlive},
        Sse,
    },
    routing::get,
};
use futures_util::Stream;
use parking_lot::Mutex;
use serde::Serialize;

use crate::{
    audio::{AudioEvent, SAMPLE_RATE},
    auth::{Session, User, UserId},
    events::Handler,
    queue::{QueueEvent, QueueId, QueueItem, SerializedQueue},
    rooms::{RoomEvent, RoomId},
    store::Store,
    track::TrackId,
    util::ID_COUNTER,
    VinylEvent,
};

use super::Router;

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "kebab-case")]
enum Message {
    /// A new user connected the room stream
    UserEnteredRoom {
        user: User,
        room: RoomId,
    },
    /// A user disconnected from the room stream
    UserLeftRoom {
        user: UserId,
        room: RoomId,
    },
    /// The current track in a room changed
    QueueAdvance {
        queue: QueueId,
        item: QueueItem,
    },
    QueueUpdate(SerializedQueue),
    /// Scheduler read a sink and set a new offset
    PlayerTime {
        room: RoomId,
        seconds: f32,
        total_seconds: f32,
    },
    /// Track activation failed
    TrackActivationError {
        queue: QueueId,
        track: TrackId,
    },
}

pub enum Recipients {
    All,
    Superuser,
    Some(Vec<UserId>),
}

pub struct SseManager {
    me: Weak<Self>,
    store: Weak<Store>,
    connections: Mutex<Vec<Arc<Connection>>>,
}

pub struct SseManagerHandler {
    store: Weak<Store>,
    manager: Weak<SseManager>,
}

pub struct Connection {
    user: User,
    handle: ConnectionHandleId,
    pending_messages: Mutex<Vec<Message>>,
    waker: Mutex<Option<Waker>>,
}

pub type ConnectionHandleId = u64;

pub struct ConnectionHandle {
    id: ConnectionHandleId,
    manager: Weak<SseManager>,
    connection: Arc<Connection>,
}

impl SseManager {
    pub fn new(store: Weak<Store>) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            store,
            me: me.clone(),
            connections: Default::default(),
        })
    }

    pub fn handler(&self) -> SseManagerHandler {
        SseManagerHandler {
            store: self.store.clone(),
            manager: self.me.clone(),
        }
    }

    fn broadcast(&self, message: Message, recipients: Recipients) {
        let connections = self.connections.lock();

        connections
            .iter()
            .filter(|x| match &recipients {
                Recipients::All => true,
                Recipients::Superuser => todo!(),
                Recipients::Some(targets) => targets.contains(&x.user.id),
            })
            .for_each(|c| c.send(message.clone()));
    }

    fn connect(&self, user: User) -> ConnectionHandle {
        let handle_id = ID_COUNTER.fetch_add(1);

        let connection = Arc::new(Connection {
            user,
            handle: handle_id,
            waker: Default::default(),
            pending_messages: Default::default(),
        });

        self.connections.lock().push(connection.clone());

        ConnectionHandle {
            manager: self.me.clone(),
            id: handle_id,
            connection,
        }
    }

    fn disconnect(&self, handle: ConnectionHandleId) {
        self.connections.lock().retain(|x| x.handle != handle);
    }
}

impl SseManagerHandler {
    fn handle_queue_event(&self, event: QueueEvent) -> Option<(Message, Recipients)> {
        match event {
            QueueEvent::Update {
                queue,
                new_items: _,
            } => {
                let queue = self.store().queue_store.serialized(queue);
                Some((Message::QueueUpdate(queue), Recipients::All))
            }
            QueueEvent::Advance { queue, item } => {
                Some((Message::QueueAdvance { queue, item }, Recipients::All))
            }
            QueueEvent::ActivationError { queue, track } => Some((
                Message::TrackActivationError { queue, track },
                Recipients::All,
            )),
        }
    }

    fn handle_audio_event(&self, event: AudioEvent) -> Option<(Message, Recipients)> {
        match event {
            AudioEvent::Time {
                player,
                total_offset,
                offset,
            } => {
                // cannot get room here yet
                let room = player.upgrade_into::<RoomId>(&self.store());

                let seconds = offset as f32 / (SAMPLE_RATE * 2) as f32;
                let total_seconds = total_offset as f32 / (SAMPLE_RATE * 2) as f32;

                Some((
                    Message::PlayerTime {
                        room,
                        seconds,
                        total_seconds,
                    },
                    Recipients::All,
                ))
            }
            _ => None,
        }
    }

    fn handle_room_event(&self, event: RoomEvent) -> Option<(Message, Recipients)> {
        match event {
            RoomEvent::UserEnteredRoom { user, room } => {
                Some((Message::UserEnteredRoom { user, room }, Recipients::All))
            }
            RoomEvent::UserLeftRoom { user, room } => {
                Some((Message::UserLeftRoom { user, room }, Recipients::All))
            }
        }
    }

    fn store(&self) -> Arc<Store> {
        self.store.upgrade().expect("store")
    }
}

impl Handler<VinylEvent> for SseManagerHandler {
    type Incoming = VinylEvent;

    fn handle(&self, incoming: Self::Incoming) {
        let response = match incoming {
            VinylEvent::Queue(event) => self.handle_queue_event(event),
            VinylEvent::Audio(event) => self.handle_audio_event(event),
            VinylEvent::Room(event) => self.handle_room_event(event),
            _ => {
                return;
            }
        };

        if let Some((message, recipients)) = response {
            self.manager
                .upgrade()
                .expect("manager upgrades")
                .broadcast(message, recipients);
        }
    }
}

impl Connection {
    fn send(&self, message: Message) {
        self.pending_messages.lock().push(message);

        if let Some(waker) = self.waker.lock().take() {
            waker.wake()
        }
    }
}

impl Stream for ConnectionHandle {
    type Item = Result<Event, Infallible>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut pending_messages = self.connection.pending_messages.lock();

        let next_event = pending_messages
            .pop()
            .map(|m| serde_json::to_string(&m).expect("serializes properly"));

        if let Some(event) = next_event {
            return Poll::Ready(Some(Ok(Event::default().data(event))));
        }

        *self.connection.waker.lock() = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl Drop for ConnectionHandle {
    fn drop(&mut self) {
        self.manager
            .upgrade()
            .expect("manager upgrades")
            .disconnect(self.id)
    }
}

pub(super) fn router() -> Router {
    Router::new().route("/", get(sse_stream))
}

async fn sse_stream(
    session: Session,
    State(context): crate::server::Context,
) -> Sse<ConnectionHandle> {
    let handle = context.sse.connect(session.user);
    Sse::new(handle).keep_alive(KeepAlive::default())
}
