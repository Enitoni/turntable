use aide::axum::{routing::get_with, IntoApiResponse};
use axum::{extract::Path, Json};

use crate::{
    auth::{with_auth, Session},
    context::ServerContext,
    errors::ServerResult,
    serialized::{Room, ToSerialized},
    Router,
};

async fn list_rooms(_session: Session, context: ServerContext) -> impl IntoApiResponse {
    let rooms: Vec<_> = context
        .collab
        .rooms
        .list_all()
        .into_iter()
        .map(|r| r.to_serialized())
        .collect();

    Json(rooms)
}

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
        .api_route(
            "/rooms",
            get_with(list_rooms, |op| {
                with_auth(op.description("Gets all rooms on this instance"))
            }),
        )
        .api_route(
            "/rooms/:slug",
            get_with(room, |op| {
                with_auth(
                    op.description("Gets a room by its slug")
                        .parameter::<String, _>("slug", |p| p),
                )
            }),
        )
        .with_path_items(|op| op.tag("rooms"))
}
