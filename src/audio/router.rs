use axum::{
    extract::State,
    routing::{get, post},
};
use hyper::{Response, StatusCode};
use log::trace;
use tokio::task::spawn_blocking;

use super::{Input, WaveStream};
use crate::{
    ingest::InputError,
    server::{Context, Router},
};

async fn get_stream(State(context): Context) -> Response<hyper::Body> {
    let stream = context.audio.stream();
    let stream = WaveStream::new(stream);

    let body = hyper::Body::wrap_stream(stream);

    Response::builder()
        .status(200)
        .header("Content-Type", WaveStream::MIME)
        .header("Content-Disposition", "inline; filename=\"stream.wav\"")
        .body(body)
        .unwrap()
}

async fn add_input(State(context): Context, query: String) -> Result<String, InputError> {
    let input = spawn_blocking(move || Input::parse(&query))
        .await
        .map_err(|x| InputError::Other(Box::new(x)))??;

    let name = input.to_string();
    let response = format!("Added {} to the queue", name);

    trace!(target: "vinyl::server", "Added {} to the queue", name);
    let _ = spawn_blocking(move || context.audio.add(input)).await;

    Ok(response)
}

pub fn router() -> Router {
    Router::new()
        .route("/input", post(add_input))
        .route("/stream", get(get_stream))
}
