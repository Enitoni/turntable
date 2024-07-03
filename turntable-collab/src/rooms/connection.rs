use std::{
    convert::Infallible,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures_util::Stream;
use turntable_core::{Consumer, Id};

use crate::{CollabContext, PrimaryKey};

use super::RoomId;

pub type RoomConnectionId = Id<RoomConnection>;

/// Represents a user's presence in a room
#[derive(Debug, Clone)]
pub struct RoomConnection {
    pub id: RoomConnectionId,
    pub user_id: PrimaryKey,
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
}

impl RoomConnection {
    pub fn new(user_id: PrimaryKey, source: String) -> Self {
        Self {
            id: RoomConnectionId::new(),
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
        stream: Arc<Consumer>,
    ) -> Self {
        Self {
            connection_id,
            room_id,
            context: context.clone(),
            stream,
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

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut buf = vec![0; 1024 * 4];
        let read = self.stream.read(&mut buf);

        match read {
            // If this ever errors, we just terminate the stream
            Err(_) => Poll::Ready(None),
            Ok(amount) => {
                // Remove the empty data from the vec, if any
                buf.drain(amount..);

                Poll::Ready(Some(Ok(buf)))
            }
        }
    }
}
