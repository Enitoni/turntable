use axum::{extract::Path, response::IntoResponse, routing::{get, post}, Json};
use turntable_collab::NewRoom;

use crate::{
    auth::Session, context::ServerContext, errors::ServerResult, schemas::{NewRoomSchema, ValidatedJson}, serialized::{Room, ToSerialized}, Router
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

#[utoipa::path(
    post,
    path = "/v1/rooms",
    tag = "rooms",
    request_body = NewRoomSchema,
    security(
        ("BearerAuth" = [])
    ),
    responses(
        (status = 200, body = Room)
    )
)]
async fn create_room(session: Session, context: ServerContext, ValidatedJson(body): ValidatedJson<NewRoomSchema>) -> ServerResult<Json<Room>> {
    let room = context.collab.rooms.create_room(NewRoom {
        slug: body.slug,
        title: body.title,
        description: body.description,
        user_id: session.user.id
    }).await?;

    Ok(Json(room.to_serialized()))
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_rooms))
        .route("/", post(create_room))
        .route("/:slug", get(room))
}
