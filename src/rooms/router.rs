use axum::{
    debug_handler,
    extract::{Path, State},
    response::Response,
    routing::{get, post},
    Json,
};
use hyper::StatusCode;
use serde::Deserialize;

use crate::{
    audio::WaveStream,
    auth::Session,
    server::{Context, Router},
    util::ApiError,
    VinylContext,
};

use super::{RawRoom, Room};

pub fn router() -> Router {
    Router::new()
        .route("/:id/stream", get(get_room_stream))
        .route("/", post(create_room))
        .route("/", get(get_rooms))
}

#[derive(Deserialize)]
struct CreateRoomBody {
    name: String,
}

#[debug_handler(state = VinylContext)]
async fn create_room(
    session: Session,
    State(context): Context,
    Json(body): Json<CreateRoomBody>,
) -> Result<(StatusCode, Json<RawRoom>), ApiError> {
    let room = context
        .rooms
        .create_room(&context.db, &session.user, body.name)
        .await?;

    Ok((StatusCode::CREATED, Json(room.into_raw())))
}

async fn get_rooms(_: Session, State(context): Context) -> Json<Vec<RawRoom>> {
    let rooms: Vec<_> = context
        .rooms
        .rooms()
        .into_iter()
        .map(Room::into_raw)
        .collect();

    Json(rooms)
}

async fn get_room_stream(
    session: Session,
    State(context): Context,
    Path(id): Path<String>,
) -> Result<Response<hyper::Body>, ApiError> {
    let room = context
        .rooms
        .rooms()
        .into_iter()
        .find(|r| r.id.id.to_string() == id)
        .ok_or(ApiError::NotFound("Room"))?;

    let stream = context.audio.stream();
    let stream = WaveStream::new(stream);

    let connection = room.connect(stream, session.user);
    let body = hyper::Body::wrap_stream(connection);

    Ok(Response::builder()
        .status(200)
        .header("Transfer-Encoding", "chunked")
        .header("Content-Type", WaveStream::MIME)
        .header("Content-Disposition", "inline; filename=\"stream.wav\"")
        .body(body)
        .unwrap())
}
