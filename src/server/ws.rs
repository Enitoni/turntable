use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Weak},
    time::Duration,
};

use futures::{
    future::join_all,
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use log::trace;
use tokio::{spawn, sync::Mutex, time::sleep};

type Incoming = SplitStream<Ws>;
type Outgoing = SplitSink<Ws, Message>;

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
    user: User,
}

pub enum Recipients {
    All,
    Superuser,
    Some(Vec<UserId>),
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

    async fn register_connection(&self, user: User, addr: SocketAddr, ws: Ws) {
        let (outgoing, incoming) = ws.split();

        trace!(target: "vinyl::server", "WebSocket connected: {}", &user.display_name);

        let new_connection = Connection {
            incoming: incoming.into(),
            outgoing: outgoing.into(),
            user,
            addr,
        }
        .into();

        self.connections.lock().await.insert(addr, new_connection);
    }

    async fn unregister_connection(&self, addr: SocketAddr) {
        trace!(target: "vinyl::server", "WebSocket disconnected: {}", addr);
        self.connections.lock().await.remove(&addr);
    }

    pub async fn broadcast(&self, message: String, recipients: Recipients) {
        let connections: Vec<_> = self
            .connections
            .lock()
            .await
            .values()
            .filter(|x| match &recipients {
                Recipients::All => true,
                Recipients::Superuser => todo!(),
                Recipients::Some(targets) => targets.contains(&x.user.id),
            })
            .map(Arc::clone)
            .collect();

        for connection in connections {
            connection
                .outgoing
                .lock()
                .await
                .send(Message::Text(message.clone()))
                .await
                .expect("send message");
        }
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

        sleep(Duration::from_millis(100)).await
    }
}

use axum::{
    extract::{
        ws::{Message, WebSocket as Ws},
        ConnectInfo, State, WebSocketUpgrade,
    },
    response::Response,
    routing::get,
};

use crate::auth::{Session, User, UserId};

use super::{Context, Router};

pub(super) fn router() -> Router {
    Router::new().route("/", get(handler))
}

async fn handler(
    session: Session,
    State(context): Context,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |ws| async move {
        context
            .websockets
            .register_connection(session.user, addr, ws)
            .await
    })
}
