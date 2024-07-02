use axum::{routing::get, Router as AxumRouter};
use context::ServerContext;
use std::{
    env,
    net::{Ipv6Addr, SocketAddr},
    sync::Arc,
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

/// The default port the server will listen on.
pub const DEFAULT_PORT: u16 = 9050;

type Router = AxumRouter<ServerContext>;

/// Starts the turntable server
pub async fn run_server(collab: Collab) {
    let context = ServerContext {
        collab: Arc::new(collab),
    };

    let port = env::var("TURNTABLE_SERVER_PORT")
        .map(|x| x.parse::<u16>().expect("Port must be a number"))
        .unwrap_or(DEFAULT_PORT);

    let addr: SocketAddr = (Ipv6Addr::UNSPECIFIED, port).into();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let version_one_router = Router::new()
        .nest("/auth", auth::router())
        .nest("/rooms", rooms::router());

    let root_router = Router::new()
        .nest("/v1", version_one_router)
        .route("/api.json", get(docs::docs))
        .with_state(context)
        .layer(cors);

    let listener = TcpListener::bind(&addr).await.expect("listens on address");

    axum::serve(listener, root_router.into_make_service())
        .await
        .unwrap();
}
