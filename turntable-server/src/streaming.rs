use axum::{body::Body, extract::{Path, Query}, response::Response, routing::get};
use serde::Deserialize;

use crate::{context::ServerContext, errors::ServerResult, Router};

#[derive(Debug, Deserialize)]
struct StreamAudioParams {
    latency: Option<u32>
}

/// Gets a live audio stream using a stream token.
#[utoipa::path(
    get, 
    path = "/v1/streams/{token}",
    tag = "streaming",
    params(
        ("token" = String, Path, description = "Stream token of a room"),
        ("latency" = Option<u32>, Query, description = "Controls the desired latency of the stream, where higher values means more latency. This is clamped to the pipeline's preload cache size.")
    ),
    responses(
        (
            status = 200,
            content_type = "application/octet-stream",
            description = "A live audio stream"
        )
    )
)]
async fn stream_audio(
    context: ServerContext,
    params: Query<StreamAudioParams>,
    Path(token): Path<String>,
) -> ServerResult<Response<Body>> {
    let handle = context.collab.rooms.connect(token, params.latency).await?;
    let content_type = handle.content_type();
    let body = Body::from_stream(handle);

    let response = Response::builder()
        .status(200)
        .header("Transfer-Encoding", "chunked")
        .header("Content-Type", content_type)
        .header("Cache-Control", "no-store")
        .body(body)
        .unwrap();

    Ok(response)
}

pub fn router() -> Router {
    Router::new().route("/:token", get(stream_audio))
}
