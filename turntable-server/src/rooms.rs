use axum::{extract::Path, response::IntoResponse, routing::{get, post}, Json};
use turntable_collab::NewRoom;

use crate::{
    auth::Session, context::ServerContext, errors::ServerResult, schemas::{NewRoomSchema, NewStreamKeySchema, ValidatedJson}, serialized::{Room, StreamKey, ToSerialized}, Router
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

#[utoipa::path(
    get, 
    path = "/v1/rooms/{id}/keys",
    tag = "rooms",
    security(
        ("BearerAuth" = [])
    ),
    responses(
        (status = 200, body = Vec<StreamKey>)
    )
)]
async fn stream_keys(session: Session, context: ServerContext, Path(room_id): Path<i32>) -> ServerResult<Json<Vec<StreamKey>>> {
    let keys = context.collab.rooms.list_stream_keys(session.user.id, room_id).await?;

    Ok(Json(keys.to_serialized()))
}

#[utoipa::path(
    post,
    path = "/v1/rooms/{id}/keys",
    tag = "rooms",
    request_body = NewStreamKeySchema,
    security(
        ("BearerAuth" = [])
    ),
    responses(
        (status = 200, body = StreamKey)
    )
)]
async fn create_stream_key(session: Session, context: ServerContext, Path(room_id): Path<i32>, ValidatedJson(body): ValidatedJson<NewStreamKeySchema>) -> ServerResult<Json<StreamKey>> {
    let new_key = context.collab.rooms.create_stream_key(room_id, session.user.id, body.source).await?;

    Ok(Json(new_key.to_serialized()))
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_rooms))
        .route("/", post(create_room))
        .route("/:id", get(room))
        .route("/:id/keys", get(stream_keys))
        .route("/:id/keys", post(create_stream_key))
}
