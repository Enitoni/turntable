use axum::{routing::get, Router as AxumRouter};
use context::ServerContext;
use log::info;
use sse::ServerSentEvents;
use std::{
    net::{Ipv6Addr, SocketAddr},
    sync::Arc,
    thread,
};
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use turntable_collab::Collab;

mod auth;
mod context;
mod docs;
mod errors;
mod rooms;
mod schemas;
mod serialized;
mod sse;
mod streaming;

type Router = AxumRouter<ServerContext>;

/// Starts the turntable server
pub async fn run_server(collab: &Arc<Collab>, port: u16) {
    let context = ServerContext {
        collab: collab.to_owned(),
        sse: ServerSentEvents::new(),
    };

    let addr: SocketAddr = (Ipv6Addr::UNSPECIFIED, port).into();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let version_one_router = Router::new()
        .nest("/auth", auth::router())
        .nest("/rooms", rooms::router())
        .nest("/streams", streaming::router())
        .nest("/events", sse::router());

    let root_router = Router::new()
        .nest("/v1", version_one_router)
        .route("/api.json", get(docs::docs))
        .with_state(context.clone())
        .layer(cors);

    let listener = TcpListener::bind(&addr).await.expect("listens on address");

    spawn_event_thread(&context);

    info!("Listening on http://localhost:{}", port);

    axum::serve(listener, root_router.into_make_service())
        .await
        .unwrap();
}

fn spawn_event_thread(context: &ServerContext) {
    let context = context.to_owned();

    let run = move || loop {
        let event = context.collab.wait_for_event();
        context.sse.broadcast(event.into());
    };

    thread::spawn(run);
}
