use futures_util::{FutureExt, Stream};
use parking_lot::Mutex;
use tokio::task;
use tokio::{runtime, task::spawn_blocking};

use super::{RoomId, RoomManager};
use crate::{audio::WaveStream, auth::User, util::ID_COUNTER};
use std::future::Future;
use std::{
    convert::Infallible,
    io::Read,
    pin::Pin,
    sync::{Arc, Weak},
    task::{Context, Poll},
};

pub type ConnectionHandleId = u64;

/// A handle to a connection containing its stream.
/// When this is dropped, it will notify the connection manager and remove the connection.
#[derive(Debug)]
pub struct ConnectionHandle {
    pub id: ConnectionHandleId,
    stream: Arc<Mutex<WaveStream>>,
    manager: Weak<RoomManager>,
    rt: runtime::Handle,
    fut: Mutex<Option<task::JoinHandle<Vec<u8>>>>,
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
            rt: runtime::Handle::current(),
            stream: Arc::new(stream.into()),
            fut: None.into(),
            manager,
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

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut fut_guard = self.fut.lock();

        let fut = fut_guard.get_or_insert_with(|| {
            let stream = Arc::clone(&self.stream);

            self.rt.spawn_blocking(move || {
                let mut buf = vec![0; 2048];
                stream.lock().read(&mut buf).ok();

                buf
            })
        });

        match fut.poll_unpin(cx) {
            Poll::Ready(result) => {
                fut_guard.take();
                Poll::Ready(Some(Ok(result.expect("infallible"))))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Connection {
    pub fn new(handle: ConnectionHandleId, room: RoomId, user: User) -> Self {
        Self { handle, room, user }
    }
}
