use axum::{body::Body, extract::Path, response::Response, routing::get};

use crate::{context::ServerContext, errors::ServerResult, Router};

#[utoipa::path(
    get, 
    path = "/v1/streams/{token}",
    tag = "streaming",
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
    Path(token): Path<String>,
) -> ServerResult<Response<Body>> {
    let handle = context.collab.rooms.connect(token).await?;
    let content_type = handle.content_type();
    let body = Body::from_stream(handle);

    let response = Response::builder()
        .status(200)
        .header("Transfer-Encoding", "chunked")
        .header("Content-Type", content_type)
        .header("Cache-Control", "no-store")
        .header("Content-Disposition", "inline; filename=\"stream.wav\"")
        .body(body)
        .unwrap();

    Ok(response)
}

pub fn router() -> Router {
    Router::new().route("/:token", get(stream_audio))
}
