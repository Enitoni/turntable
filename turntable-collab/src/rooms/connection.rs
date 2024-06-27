use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures_util::Stream;
use turntable_core::{Consumer, Id, Ingestion};

use crate::{CollabContext, Database, PrimaryKey};

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
pub struct RoomConnectionHandle<I, Db>
where
    I: Ingestion,
    Db: Database,
{
    connection_id: RoomConnectionId,
    room_id: RoomId,
    context: CollabContext<I, Db>,
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

impl<I, Db> RoomConnectionHandle<I, Db>
where
    I: Ingestion,
    Db: Database,
{
    pub fn new(
        context: &CollabContext<I, Db>,
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
}

impl<I, Db> Drop for RoomConnectionHandle<I, Db>
where
    I: Ingestion,
    Db: Database,
{
    fn drop(&mut self) {
        if let Some(room) = self.context.rooms.get(&self.room_id) {
            room.remove_connection(self.connection_id)
        }
    }
}

impl<I, Db> Stream for RoomConnectionHandle<I, Db>
where
    I: Ingestion,
    Db: Database,
{
    type Item = Vec<u8>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut buf = vec![0; 2048];
        let read = self.stream.read(&mut buf);

        match read {
            // If this ever errors, we just terminate the stream
            Err(_) => Poll::Ready(None),
            Ok(amount) => {
                // Remove the empty data from the vec, if any
                buf.drain(amount..);

                Poll::Ready(Some(buf))
            }
        }
    }
}
