use axum::{
    debug_handler,
    extract::{Path, State},
    response::Response,
    routing::{get, post},
    Json,
};
use hyper::StatusCode;
use log::trace;
use serde::Deserialize;
use tokio::task::spawn_blocking;

use crate::{
    audio::WaveStream,
    auth::Session,
    ingest::Input,
    server::{Context, Router},
    util::ApiError,
    VinylContext,
};

use super::{Room, RoomData};

pub fn router() -> Router {
    Router::new()
        .route("/:id/stream", get(get_room_stream))
        .route("/:id/input", post(add_input))
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
) -> Result<(StatusCode, Json<RoomData>), ApiError> {
    let room = context
        .rooms
        .create_room(&context.db, &session.user, body.name)
        .await?;

    Ok((StatusCode::CREATED, Json(room.into_data())))
}

async fn get_rooms(_: Session, State(context): Context) -> Json<Vec<RoomData>> {
    let rooms: Vec<_> = context
        .rooms
        .rooms()
        .into_iter()
        .map(Room::into_data)
        .collect();

    Json(rooms)
}

async fn add_input(
    session: Session,
    State(context): Context,
    Path(id): Path<String>,
    query: String,
) -> Result<String, ApiError> {
    let room = context
        .rooms
        .rooms()
        .into_iter()
        .find(|r| r.id.id.to_string() == id)
        .ok_or(ApiError::NotFound("Room"))?;

    let input = spawn_blocking(move || Input::parse(&query))
        .await
        .unwrap()
        .map_err(|x| ApiError::Other(Box::new(x)))?;

    let name = input.to_string();
    let response = format!("Added {} to the queue", name);

    trace!(target: "vinyl::server", "Added {} to the queue", name);
    let _ = spawn_blocking(move || context.rooms.add_input(session.user, &room.id, input)).await;

    Ok(response)
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

    let connection = context.rooms.connect(session.user, &room.id);
    let body = hyper::Body::wrap_stream(connection);

    Ok(Response::builder()
        .status(200)
        .header("Transfer-Encoding", "chunked")
        .header("Content-Type", WaveStream::MIME)
        .header("Content-Disposition", "inline; filename=\"stream.wav\"")
        .body(body)
        .unwrap())
}
