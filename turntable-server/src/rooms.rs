use axum::{extract::Path, response::IntoResponse, routing::{get, post}, Json};
use turntable_collab::{Input, NewRoom, Track as CollabTrack};
use turntable_core::Queue as CoreQueue;

use crate::{
    auth::Session,
    context::ServerContext,
    errors::ServerResult,
    schemas::{
        InputSchema, JoinWithInviteSchema, NewRoomSchema, NewStreamKeySchema, RoomActionSchema, ValidatedJson
    },
    serialized::{Queue, Room, RoomInvite, StreamKey, ToSerialized}, Router
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

#[utoipa::path(
    get, 
    path = "/v1/rooms/{id}/queue",
    tag = "rooms",
    security(
        ("BearerAuth" = [])
    ),
    responses(
        (status = 200, body = Queue)
    )
)]
async fn queue(_session: Session, context: ServerContext, Path(room_id): Path<i32>) -> ServerResult<Json<Queue>> {
    let room = context.collab.rooms.room_by_id(room_id)?;
    let queue = room.queue()?;

    Ok(Json(queue.tracks().to_serialized()))
}

#[utoipa::path(
    post,
    path = "/v1/rooms/{id}/queue",
    tag = "rooms",
    request_body = InputSchema,
    security(
        ("BearerAuth" = [])
    ),
    responses(
        (status = 200, description = "Item(s) were added to the queue")
    )
)]
async fn add_to_queue(_session: Session, context: ServerContext, Path(room_id): Path<i32>, ValidatedJson(body): ValidatedJson<InputSchema>) -> ServerResult<()> {
    let room = context.collab.rooms.room_by_id(room_id)?;
    let queue = room.queue()?;

    let input = Input::query(&body.query).await?;
    let tracks: Vec<CollabTrack> = input.into_iter().map(Into::into).collect();

    for track in tracks {
        queue.push(track)
    }

    Ok(())
}

#[utoipa::path(
    get, 
    path = "/v1/rooms/invites/{token}",
    tag = "rooms",
    responses(
        (status = 200, body = RoomInvite)
    )
)]
async fn invite_by_token(context: ServerContext, Path(token): Path<String>) -> ServerResult<Json<RoomInvite>> {
    let invite = context.collab.rooms.invite_by_token(token).await?;

    Ok(Json(invite.to_serialized()))
}

#[utoipa::path(
    post,
    path = "/v1/rooms/{id}/invites",
    tag = "rooms",
    security(
        ("BearerAuth" = [])
    ),
    responses(
        (status = 200, body = RoomInvite)
    )
)]
async fn create_invite(session: Session, context: ServerContext, Path(room_id): Path<i32>) -> ServerResult<Json<RoomInvite>> {
    let invite = context.collab.rooms.create_invite(session.user.id, room_id).await?;

    Ok(Json(invite.to_serialized()))
}

#[utoipa::path(
    post,
    path = "/v1/rooms/members",
    tag = "rooms",
    request_body = JoinWithInviteSchema,
    security(
        ("BearerAuth" = [])
    ),
    responses(
        (status = 200, description = "User was added as member to room, and invite is consumed.")
    )
)]
async fn join_with_invite(session: Session, context: ServerContext, ValidatedJson(body): ValidatedJson<JoinWithInviteSchema>) -> ServerResult<()> {
    context.collab.rooms.add_member_with_invite(session.user.id, body.token).await?;
    Ok(())
}

#[utoipa::path(
    post,
    path = "/v1/rooms/{id}/actions",
    tag = "rooms",
    request_body = RoomActionSchema,
    security(
        ("BearerAuth" = [])
    ),
    responses(
        (status = 200, description = "Action was performed.")
    )
)]
async fn perform_room_action(_session: Session, context: ServerContext, Path(room_id): Path<i32>, Json(body): Json<RoomActionSchema>) -> ServerResult<()> {
    let room = context.collab.rooms.room_by_id(room_id)?;

    match body {
        RoomActionSchema::Play => { room.player()?.play() },
        RoomActionSchema::Pause => { room.player()?.pause() },
        RoomActionSchema::Next => { room.queue()?.next() },
        RoomActionSchema::Previous => { room.queue()?.previous() },
        RoomActionSchema::Seek { to } => { room.player()?.seek(to) }
    };

    Ok(())
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_rooms))
        .route("/", post(create_room))
        .route("/invites/:id", get(invite_by_token))
        .route("/members", post(join_with_invite))
        .route("/:id", get(room))
        .route("/:id/keys", get(stream_keys))
        .route("/:id/keys", post(create_stream_key))
        .route("/:id/queue", get(queue))
        .route("/:id/queue", post(add_to_queue))
        .route("/:id/invites", post(create_invite))
        .route("/:id/actions", post(perform_room_action))
}
