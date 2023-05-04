use axum::{extract::State, Router as AxumRouter};
use std::{env, net::SocketAddr};
use tower_http::cors::{Any, CorsLayer};

use crate::{audio, auth, VinylContext};
pub mod ws;

pub const DEFAULT_PORT: u16 = 9050;
pub type Router = AxumRouter<VinylContext>;
pub type Context = State<VinylContext>;

pub async fn run_server(context: VinylContext) {
    context.websockets.run().await;

    let port = env::var("VINYL_SERVER_PORT")
        .map(|x| x.parse::<u16>().expect("Port must be a number"))
        .unwrap_or(DEFAULT_PORT);

    let addr = ([127, 0, 0, 1], port).into();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let version_one_router = AxumRouter::new()
        .nest("/auth", auth::router())
        .nest("/gateway", ws::router())
        .nest("/audio", audio::router());

    let router = AxumRouter::new()
        .nest("/v1", version_one_router)
        .with_state(context)
        .layer(cors);

    axum::Server::bind(&addr)
        .serve(router.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}
