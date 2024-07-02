use axum::{extract::Path, response::IntoResponse, routing::get, Json};

use crate::{
    auth::Session,
    context::ServerContext,
    errors::ServerResult,
    serialized::{Room, ToSerialized},
    Router,
};

#[utoipa::path(
    get, 
    path = "/v1/rooms",
    tag = "rooms",
    security(
        ("BearerAuth" = [])
    ),
    responses(
        (status = 200, body = Vec<Room>)
    )
)]
async fn list_rooms(_session: Session, context: ServerContext) -> impl IntoResponse {
    let rooms: Vec<_> = context
        .collab
        .rooms
        .list_all()
        .into_iter()
        .map(|r| r.to_serialized())
        .collect();

    Json(rooms)
}

#[utoipa::path(
    get, 
    path = "/v1/rooms/{slug}",
    tag = "rooms",
    security(
        ("BearerAuth" = [])
    ),
    responses(
        (status = 200, body = Room)
    )
)]
async fn room(
    _session: Session,
    context: ServerContext,
    Path(slug): Path<String>,
) -> ServerResult<Json<Room>> {
    let room = context.collab.rooms.room_by_slug(&slug)?;

    Ok(Json(room.to_serialized()))
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_rooms))
        .route("/:slug", get(room))
}
