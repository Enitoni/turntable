use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Weak},
};

use futures::{
    future::join_all,
    stream::{SplitSink, SplitStream},
    StreamExt,
};
use log::trace;
use tokio::{spawn, sync::Mutex};

type Incoming = SplitStream<WebSocket>;
type Outgoing = SplitSink<WebSocket, Message>;

#[derive(Debug)]
pub struct WebSocketManager {
    me: Weak<WebSocketManager>,
    connections: Mutex<HashMap<SocketAddr, Arc<Connection>>>,
}

#[derive(Debug)]
struct Connection {
    addr: SocketAddr,
    incoming: Mutex<Incoming>,
    outgoing: Mutex<Outgoing>,
}

impl WebSocketManager {
    pub fn new() -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            connections: Default::default(),
        })
    }

    pub(super) async fn run(&self) {
        spawn(check_connections(
            self.me.upgrade().expect("upgrade weak to arc"),
        ));
    }

    async fn register_connection(&self, addr: SocketAddr, ws: WebSocket) {
        let (outgoing, incoming) = ws.split();
        let new_connection = Connection {
            incoming: incoming.into(),
            outgoing: outgoing.into(),
            addr,
        }
        .into();

        trace!(target: "vinyl::server", "WebSocket connected: {}", addr);
        self.connections.lock().await.insert(addr, new_connection);
    }

    async fn unregister_connection(&self, addr: SocketAddr) {
        trace!(target: "vinyl::server", "WebSocket disconnected: {}", addr);
        self.connections.lock().await.remove(&addr);
    }
}

/// Handle when connections drop
async fn check_connections(manager: Arc<WebSocketManager>) {
    loop {
        let futures: Vec<_> = manager
            .connections
            .lock()
            .await
            .values()
            .cloned()
            .map(|c| async move { c.incoming.lock().await.next().await.map(|v| (c.addr, v)) })
            .collect();

        let messages: Vec<_> = join_all(futures).await.into_iter().flatten().collect();

        for (addr, message) in messages {
            match message {
                Ok(Message::Close(_)) => manager.unregister_connection(addr).await,
                Err(err) => {
                    dbg!(err);
                }
                _ => {}
            }
        }
    }
}

use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    response::Response,
    routing::get,
};

use super::{Context, Router};

pub(super) fn router() -> Router {
    Router::new().route("/", get(handler))
}

async fn handler(
    State(context): Context,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |ws| async move { context.websockets.register_connection(addr, ws).await })
}
