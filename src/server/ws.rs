use std::{
    collections::HashMap,
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, Weak},
};

use futures::{
    future::join_all,
    stream::{SplitSink, SplitStream},
    StreamExt,
};
use tokio::{spawn, sync::Mutex};
use warp::{
    reject::Reject,
    ws::{Message, WebSocket, Ws},
    Filter, Rejection, Reply,
};

use crate::VinylContext;

use super::with_state;

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

        self.connections.lock().await.insert(addr, new_connection);
    }

    async fn unregister_connection(&self, addr: SocketAddr) {
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
                Ok(message) => {
                    if message.is_close() {
                        manager.unregister_connection(addr).await
                    }
                }
                Err(err) => {
                    dbg!(err);
                }
            }
        }
    }
}

#[derive(Debug)]
struct SocketAddrRequired;
impl Reject for SocketAddrRequired {}

pub(super) fn routes(
    context: VinylContext,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    warp::path("gateway")
        .and(warp::addr::remote())
        .and_then(
            |addr: Option<_>| async move { addr.ok_or(warp::reject::custom(SocketAddrRequired)) },
        )
        .and(warp::ws())
        .and(with_state(context))
        .map(|addr: SocketAddr, ws: Ws, context: VinylContext| {
            ws.on_upgrade(move |ws| async move {
                context.websockets.register_connection(addr, ws).await
            })
        })
}
