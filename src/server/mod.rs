use std::{
    convert::Infallible,
    sync::Arc,
    thread::{self, JoinHandle},
};

use warp::Filter;

use self::ws::WebSocketManager;
mod ws;

pub const DEFAULT_PORT: u16 = 9050;

#[derive(Debug, Clone)]
pub(self) struct ServerContext {
    ws: Arc<WebSocketManager>,
}

async fn start_server_inner(context: ServerContext) {
    context.ws.run().await;

    let root = warp::path("v1").and(ws::routes(context));

    warp::serve(root).run(([127, 0, 0, 1], DEFAULT_PORT)).await
}

pub fn start_server() -> JoinHandle<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let context = ServerContext {
        ws: WebSocketManager::new(),
    };

    thread::spawn(move || rt.block_on(start_server_inner(context)))
}

pub(self) fn with_state<T>(state: T) -> impl Filter<Extract = (T,), Error = Infallible> + Clone
where
    T: Clone + Send,
{
    warp::any().map(move || state.clone())
}
