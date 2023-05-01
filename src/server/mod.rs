use std::{
    convert::Infallible,
    sync::Arc,
    thread::{self, JoinHandle},
};

use warp::Filter;

use crate::VinylContext;
pub mod ws;

pub const DEFAULT_PORT: u16 = 9050;

async fn start_server_inner(context: VinylContext) {
    context.websockets.run().await;

    let root = warp::path("v1").and(ws::routes(context));

    warp::serve(root).run(([127, 0, 0, 1], DEFAULT_PORT)).await
}

pub fn run_server(context: VinylContext) -> JoinHandle<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    thread::spawn(move || rt.block_on(start_server_inner(context)))
}

pub(self) fn with_state<T>(state: T) -> impl Filter<Extract = (T,), Error = Infallible> + Clone
where
    T: Clone + Send,
{
    warp::any().map(move || state.clone())
}
