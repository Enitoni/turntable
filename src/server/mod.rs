use axum::{extract::State, Router as AxumRouter};
use std::{
    env,
    net::{Ipv6Addr, SocketAddr},
};
use tower_http::cors::{Any, CorsLayer};

use crate::{auth, rooms, VinylContext};

pub mod sse;

pub const DEFAULT_PORT: u16 = 9050;
pub type Router = AxumRouter<VinylContext>;
pub type Context = State<VinylContext>;

pub async fn run_server(context: VinylContext) {
    let port = env::var("VINYL_SERVER_PORT")
        .map(|x| x.parse::<u16>().expect("Port must be a number"))
        .unwrap_or(DEFAULT_PORT);

    let addr = (Ipv6Addr::UNSPECIFIED, port).into();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let version_one_router = AxumRouter::new()
        .nest("/auth", auth::router())
        .nest("/events", sse::router())
        .nest("/rooms", rooms::router());

    let router = AxumRouter::new()
        .nest("/v1", version_one_router)
        .with_state(context)
        .layer(cors);

    axum::Server::bind(&addr)
        .serve(router.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}
