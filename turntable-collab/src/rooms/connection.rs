use std::{
    convert::Infallible,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures_util::{FutureExt, Stream};
use parking_lot::Mutex;
use tokio::task::{spawn_blocking, JoinHandle};
use turntable_core::{Consumer, ConsumerId, Id};

use crate::{CollabContext, PrimaryKey};

use super::RoomId;

pub type RoomConnectionId = Id<RoomConnection>;

/// Represents a user's presence in a room
#[derive(Debug, Clone)]
pub struct RoomConnection {
    pub id: RoomConnectionId,
    pub user_id: PrimaryKey,
    pub consumer_id: ConsumerId,
    /// Same as StreamKey source.
    pub source: String,
}

/// A handle to a stream, which when dropped removes the [RoomConnection] from a room
pub struct RoomConnectionHandle {
    connection_id: RoomConnectionId,
    room_id: RoomId,
    context: CollabContext,
    /// The audio stream
    stream: Arc<Consumer>,
    /// The future being polled currently
    fut: Mutex<Option<JoinHandle<Option<Vec<u8>>>>>,
}

impl RoomConnection {
    pub fn new(user_id: PrimaryKey, consumer_id: ConsumerId, source: String) -> Self {
        Self {
            id: RoomConnectionId::new(),
            consumer_id,
            user_id,
            source,
        }
    }
}

impl RoomConnectionHandle {
    pub fn new(
        context: &CollabContext,
        connection_id: RoomConnectionId,
        room_id: RoomId,
        stream: Consumer,
    ) -> Self {
        Self {
            connection_id,
            room_id,
            context: context.clone(),
            fut: Default::default(),
            stream: stream.into(),
        }
    }

    /// Get the content type of the stream
    pub fn content_type(&self) -> String {
        self.stream.content_type()
    }
}

impl Drop for RoomConnectionHandle {
    fn drop(&mut self) {
        if let Some(room) = self.context.rooms.get(&self.room_id) {
            room.remove_connection(self.connection_id)
        }
    }
}

impl Stream for RoomConnectionHandle {
    type Item = Result<Vec<u8>, Infallible>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut fut_guard = self.fut.lock();
        let cloned_stream = self.stream.clone();

        let fut = fut_guard.get_or_insert_with(|| spawn_blocking(move || cloned_stream.bytes()));

        match fut.poll_unpin(cx) {
            Poll::Ready(result) => {
                fut_guard.take();

                let maybe_bytes = result.expect("infallible");

                match maybe_bytes {
                    Some(bytes) => Poll::Ready(Some(Ok(bytes))),
                    None => Poll::Ready(None),
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
