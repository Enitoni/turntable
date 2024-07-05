use axum::{
    response::{
        sse::{Event, KeepAlive},
        Sse,
    },
    routing::get,
};
use futures_util::Stream;
use parking_lot::Mutex;
use serde::Serialize;
use std::{
    convert::Infallible,
    pin::Pin,
    sync::{Arc, Weak},
    task::{Context, Poll, Waker},
};
use turntable_collab::CollabEvent;
use turntable_core::Id;
use utoipa::ToSchema;

use crate::{
    context::ServerContext,
    serialized::{PlayerState, QueueItem, RoomMember, ToSerialized},
    Router,
};

type ConnectionId = Id<Connection>;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case", tag = "type")]
pub enum ServerEvent {
    /// A player's state has changed.
    PlayerStateUpdate {
        room_id: i32,
        new_state: PlayerState,
    },
    /// A player's playback offset has changed.
    PlayerTimeUpdate {
        room_id: i32,
        /// The current position of the player, in seconds.
        position: f32,
        /// The total position of the player, in seconds.
        total_position: f32,
    },
    /// A track as a queue item has been ingested
    TrackActivated {
        /// The id of the player the queue item's queue belongs to.
        room_id: i32,
        /// The id of the new sink created for the queue item.
        track_id: i32,
    },
    /// A track as a queue item failed to be ingested.
    TrackActivationError {
        /// The id of the player the queue item's queue belongs to.
        room_id: i32,
        /// The id of the new sink created for the queue item.
        track_id: i32,
        /// The error that happened while activating the queue item.
        error: String,
    },
    /// The currently playing track of a room updated
    RoomQueueItemUpdate {
        room_id: i32,
        new_item: Option<QueueItem>,
    },
    /// A queue was modified and updated
    RoomQueueUpdate {
        room_id: i32,
        history: Vec<QueueItem>,
        items: Vec<QueueItem>,
    },
    /// User become a member of a room
    UserJoined {
        room_id: i32,
        new_member: RoomMember,
    },
    /// User left a room
    UserLeft { room_id: i32, member_id: i32 },
    /// A user connected with a stream key to a room
    UserConnected {
        room_id: i32,
        user_id: i32,
        source: String,
    },
    /// A user disconnected from a room
    UserDisconnected {
        room_id: i32,
        user_id: i32,
        source: String,
    },
}

impl From<CollabEvent> for ServerEvent {
    fn from(value: CollabEvent) -> Self {
        match value {
            CollabEvent::PlayerStateUpdate { room_id, new_state } => Self::PlayerStateUpdate {
                room_id,
                new_state: new_state.to_serialized(),
            },
            CollabEvent::PlayerTimeUpdate {
                room_id,
                position,
                total_position,
            } => Self::PlayerTimeUpdate {
                room_id,
                position,
                total_position,
            },
            CollabEvent::RoomQueueItemUpdate { room_id, new_item } => Self::RoomQueueItemUpdate {
                room_id,
                new_item: new_item.to_serialized(),
            },
            CollabEvent::RoomQueueUpdate {
                room_id,
                history,
                items,
            } => Self::RoomQueueUpdate {
                room_id,
                history: history.to_serialized(),
                items: items.to_serialized(),
            },
            CollabEvent::TrackActivated { room_id, track_id } => Self::TrackActivated {
                room_id,
                track_id: track_id.value() as i32,
            },
            CollabEvent::TrackActivationError {
                room_id,
                track_id,
                error,
            } => Self::TrackActivationError {
                room_id,
                track_id: track_id.value() as i32,
                error,
            },
            CollabEvent::UserConnected {
                room_id,
                user_id,
                source,
            } => Self::UserConnected {
                room_id,
                user_id,
                source,
            },
            CollabEvent::UserDisconnected {
                room_id,
                user_id,
                source,
            } => Self::UserDisconnected {
                room_id,
                user_id,
                source,
            },
            CollabEvent::UserJoined {
                room_id,
                new_member,
            } => Self::UserJoined {
                room_id,
                new_member: new_member.to_serialized(),
            },
            CollabEvent::UserLeft { room_id, member_id } => Self::UserLeft { room_id, member_id },
        }
    }
}

/// Manages server sent event connections
pub struct ServerSentEvents {
    me: Weak<Self>,
    connections: Mutex<Vec<Connection>>,
}

struct Connection {
    id: ConnectionId,
    pending_messages: Arc<Mutex<Vec<ServerEvent>>>,
    waker: Arc<Mutex<Option<Waker>>>,
}

struct ConnectionHandle {
    id: ConnectionId,
    /// A reference to [Connection]'s pending messages
    pending_messages: Arc<Mutex<Vec<ServerEvent>>>,
    /// A reference to [Connection]'s stored [Waker]
    waker: Arc<Mutex<Option<Waker>>>,
    /// Required to remove connection when dropped
    manager: Weak<ServerSentEvents>,
}

impl ServerSentEvents {
    pub fn new() -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            connections: Default::default(),
        })
    }

    pub fn broadcast(&self, event: ServerEvent) {
        let connections = self.connections.lock();

        for connection in connections.iter() {
            connection.send(event.clone())
        }
    }

    fn connect(&self) -> ConnectionHandle {
        let connection = Connection::new();
        let handle = connection.handle(self.me.clone());

        self.connections.lock().push(connection);
        handle
    }

    fn disconnect(&self, id: ConnectionId) {
        self.connections.lock().retain(|c| c.id != id)
    }
}

impl Connection {
    fn new() -> Self {
        Self {
            id: ConnectionId::new(),
            pending_messages: Default::default(),
            waker: Default::default(),
        }
    }

    fn send(&self, message: ServerEvent) {
        self.pending_messages.lock().push(message);

        if let Some(waker) = self.waker.lock().take() {
            waker.wake()
        }
    }

    fn handle(&self, manager: Weak<ServerSentEvents>) -> ConnectionHandle {
        ConnectionHandle {
            id: self.id,
            pending_messages: self.pending_messages.clone(),
            waker: self.waker.clone(),
            manager,
        }
    }
}

impl Stream for ConnectionHandle {
    type Item = Result<Event, Infallible>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut pending_messages = self.pending_messages.lock();

        let next_event = pending_messages
            .pop()
            .map(|m| serde_json::to_string(&m).expect("serializes properly"));

        if let Some(event) = next_event {
            return Poll::Ready(Some(Ok(Event::default().data(event))));
        }

        *self.waker.lock() = Some(cx.waker().clone());
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

#[utoipa::path(
    get, 
    path = "/v1/events",
    tag = "events",
    responses(
        (
            status = 200,
            content_type = "text/event-stream",
            description = "A stream of events from turntable",
            body = ServerEvent
        )
    )
)]
async fn event_stream(context: ServerContext) -> Sse<ConnectionHandle> {
    Sse::new(context.sse.connect()).keep_alive(KeepAlive::default())
}

pub fn router() -> Router {
    Router::new().route("/", get(event_stream))
}
