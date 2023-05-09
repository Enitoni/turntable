use futures_util::Stream;

use super::{RoomId, RoomManager};
use crate::{audio::WaveStream, auth::User, util::ID_COUNTER};
use std::{
    convert::Infallible,
    io::Read,
    pin::Pin,
    sync::Weak,
    task::{Context, Poll},
};

pub type ConnectionHandleId = u64;

/// A handle to a connection containing its stream.
/// When this is dropped, it will notify the connection manager and remove the connection.
#[derive(Debug)]
pub struct ConnectionHandle {
    pub id: ConnectionHandleId,
    stream: WaveStream,
    manager: Weak<RoomManager>,
}

/// A connection to a room.
#[derive(Debug)]
pub struct Connection {
    /// The id to the handle used to remove this connection when it is dropped
    pub handle: ConnectionHandleId,
    /// The room this connection belongs to
    pub room: RoomId,
    /// The user this connection was made by
    pub user: User,
}

impl ConnectionHandle {
    pub fn new(manager: Weak<RoomManager>, stream: WaveStream) -> Self {
        Self {
            id: ID_COUNTER.fetch_add(1),
            manager,
            stream,
        }
    }
}

impl Drop for ConnectionHandle {
    fn drop(&mut self) {
        self.manager
            .upgrade()
            .expect("manager is never none")
            .notify_disconnect(self.id)
    }
}

impl Stream for ConnectionHandle {
    type Item = Result<Vec<u8>, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut buf = vec![0; 2048];
        self.stream.read(&mut buf).ok();

        Poll::Ready(Some(Ok(buf)))
    }
}

impl Connection {
    pub fn new(handle: ConnectionHandleId, room: RoomId, user: User) -> Self {
        Self { handle, room, user }
    }
}
