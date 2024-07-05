//! All schemas that are exposed from endpoints are defined here
//! along with the From<T> impls

use std::sync::Arc;

use serde::Serialize;
use turntable_collab::{
    LinearQueueItem, Room as CollabRoom, RoomConnection as CollabRoomConnection, RoomInviteData,
    RoomMemberData, SessionData, StreamKeyData, Track as CollabTrack, UserData,
};
use turntable_core::PlayerState as CorePlayerState;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct User {
    id: i32,
    username: String,
    display_name: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginResult {
    token: String,
    user: User,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    id: i32,
    title: String,
    description: Option<String>,
    members: Vec<RoomMember>,
    connections: Vec<RoomConnection>,
    player: Option<Player>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoomMember {
    id: i32,
    owner: bool,
    user: User,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoomConnection {
    user_id: i32,
    source: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoomInvite {
    token: String,
    inviter: User,
    room_title: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StreamKey {
    id: i32,
    token: String,
    source: String,
    room_id: i32,
    user_id: i32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    id: i32,
    title: String,
    artist: String,

    canonical: String,
    source: String,

    duration: f32,
    artwork: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    user_id: i32,
    track: Track,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Queue {
    items: Vec<QueueItem>,
    history: Vec<QueueItem>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    state: PlayerState,
    total_time: f32,
    current_time: f32,
    current_item: Option<QueueItem>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PlayerState {
    Idle,
    Playing,
    Buffering,
}

/// Helper trait to convert any type into a serialized version
pub trait ToSerialized<T>
where
    T: Serialize,
{
    fn to_serialized(&self) -> T;
}

impl<I, O> ToSerialized<Vec<O>> for Vec<I>
where
    I: ToSerialized<O>,
    O: Serialize,
{
    fn to_serialized(&self) -> Vec<O> {
        self.iter().map(|x| x.to_serialized()).collect()
    }
}

impl<I, O> ToSerialized<Option<O>> for Option<I>
where
    I: ToSerialized<O> + Clone,
    O: Serialize,
{
    fn to_serialized(&self) -> Option<O> {
        self.clone().map(|t| t.to_serialized())
    }
}

impl ToSerialized<User> for UserData {
    fn to_serialized(&self) -> User {
        User {
            id: self.id,
            username: self.username.clone(),
            display_name: self.display_name.clone(),
        }
    }
}

impl ToSerialized<LoginResult> for SessionData {
    fn to_serialized(&self) -> LoginResult {
        LoginResult {
            token: self.token.clone(),
            user: self.user.to_serialized(),
        }
    }
}

impl ToSerialized<Room> for Arc<CollabRoom> {
    fn to_serialized(&self) -> Room {
        let data = self.data();

        let track = self.current_item();
        let player = self
            .player()
            .map(|p| Player {
                current_time: p.current_time(),
                total_time: p.current_total_time(),
                current_item: track.map(|t| t.to_serialized()),
                state: p.current_state().to_serialized(),
            })
            .ok();

        Room {
            id: data.id,
            title: data.title,
            description: data.description,
            members: data.members.to_serialized(),
            connections: self.current_connections().to_serialized(),
            player,
        }
    }
}

impl ToSerialized<RoomMember> for RoomMemberData {
    fn to_serialized(&self) -> RoomMember {
        RoomMember {
            id: self.id,
            owner: self.owner,
            user: self.user.to_serialized(),
        }
    }
}

impl ToSerialized<RoomConnection> for CollabRoomConnection {
    fn to_serialized(&self) -> RoomConnection {
        RoomConnection {
            user_id: self.user_id,
            source: self.source.clone(),
        }
    }
}

impl ToSerialized<StreamKey> for StreamKeyData {
    fn to_serialized(&self) -> StreamKey {
        StreamKey {
            id: self.id,
            token: self.token.clone(),
            source: self.source.clone(),
            room_id: self.room_id,
            user_id: self.user_id,
        }
    }
}

impl ToSerialized<RoomInvite> for RoomInviteData {
    fn to_serialized(&self) -> RoomInvite {
        RoomInvite {
            token: self.token.clone(),
            inviter: self.inviter.to_serialized(),
            room_title: self.room.title.clone(),
        }
    }
}

impl ToSerialized<Track> for CollabTrack {
    fn to_serialized(&self) -> Track {
        Track {
            id: self.id.value() as i32,
            title: self.metadata.title.clone(),
            artwork: self.metadata.artwork.clone(),
            canonical: self.metadata.canonical.clone(),
            source: self.metadata.source.clone(),
            duration: self.metadata.duration,
            artist: self
                .metadata
                .artist
                .clone()
                .unwrap_or_else(|| "Unknown artist".to_string()),
        }
    }
}

impl ToSerialized<QueueItem> for LinearQueueItem {
    fn to_serialized(&self) -> QueueItem {
        QueueItem {
            user_id: self.user_id,
            track: self.track.to_serialized(),
        }
    }
}

impl ToSerialized<Queue> for (Vec<LinearQueueItem>, Vec<LinearQueueItem>) {
    fn to_serialized(&self) -> Queue {
        Queue {
            items: self.0.to_serialized(),
            history: self.1.to_serialized(),
        }
    }
}

impl ToSerialized<PlayerState> for CorePlayerState {
    fn to_serialized(&self) -> PlayerState {
        match self {
            Self::Idle => PlayerState::Idle,
            Self::Playing => PlayerState::Playing,
            Self::Buffering => PlayerState::Buffering,
        }
    }
}
