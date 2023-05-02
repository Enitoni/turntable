use std::{
    convert::Infallible,
    sync::Arc,
    thread::{self, JoinHandle},
};

use warp::Filter;

use crate::{audio, VinylContext};
pub mod ws;

pub const DEFAULT_PORT: u16 = 9050;

pub async fn run_server(context: VinylContext) {
    context.websockets.run().await;

    let root = warp::path("v1").and(ws::routes(context.clone()).or(audio::routes(context)));

    warp::serve(root).run(([127, 0, 0, 1], DEFAULT_PORT)).await
}

pub fn with_state<T>(state: T) -> impl Filter<Extract = (T,), Error = Infallible> + Clone
where
    T: Clone + Send,
{
    warp::any().map(move || state.clone())
}
