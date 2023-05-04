use axum::{extract::State, Router as AxumRouter};
use std::net::SocketAddr;

use crate::{audio, auth, VinylContext};
pub mod ws;

pub const DEFAULT_PORT: u16 = 9050;
pub type Router = AxumRouter<VinylContext>;
pub type Context = State<VinylContext>;

pub async fn run_server(context: VinylContext) {
    context.websockets.run().await;

    let addr = ([127, 0, 0, 1], DEFAULT_PORT).into();

    let version_one_router = AxumRouter::new()
        .nest("/auth", auth::router())
        .nest("/gateway", ws::router())
        .nest("/audio", audio::router());

    let router = AxumRouter::new()
        .nest("/v1", version_one_router)
        .with_state(context);

    axum::Server::bind(&addr)
        .serve(router.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}
