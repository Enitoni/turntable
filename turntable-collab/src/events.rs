use crossbeam::channel::{Receiver, Sender};
use turntable_core::{PipelineEvent, PlayerState};

use crate::{CollabContext, LinearQueueItem, PrimaryKey, RoomMemberData, TrackId};

pub type EventSender = Sender<CollabEvent>;
pub type EventReceiver = Receiver<CollabEvent>;

/// Events emitted by
#[derive(Debug)]
pub enum CollabEvent {
    /// A player's state has changed.
    PlayerStateUpdate {
        room_id: PrimaryKey,
        new_state: PlayerState,
    },
    /// A player's playback offset has changed.
    PlayerTimeUpdate {
        room_id: PrimaryKey,
        /// The current position of the player, in seconds.
        position: f32,
        /// The total position of the player, in seconds.
        total_position: f32,
    },
    /// A track as a queue item has been ingested
    TrackActivated {
        /// The id of the player the queue item's queue belongs to.
        room_id: PrimaryKey,
        /// The id of the new sink created for the queue item.
        track_id: TrackId,
    },
    /// A track as a queue item failed to be ingested.
    TrackActivationError {
        /// The id of the player the queue item's queue belongs to.
        room_id: PrimaryKey,
        /// The id of the new sink created for the queue item.
        track_id: TrackId,
        /// The error that happened while activating the queue item.
        error: String,
    },
    /// The currently playing track of a room updated
    RoomQueueItemUpdate {
        room_id: PrimaryKey,
        new_item: Option<LinearQueueItem>,
    },
    /// A queue was modified and updated
    RoomQueueUpdate {
        room_id: PrimaryKey,
        history: Vec<LinearQueueItem>,
        items: Vec<LinearQueueItem>,
    },
    /// User become a member of a room
    UserJoined {
        room_id: PrimaryKey,
        new_member: RoomMemberData,
    },
    /// User left a room
    UserLeft {
        room_id: PrimaryKey,
        member_id: PrimaryKey,
    },
    /// A user connected with a stream key to a room
    UserConnected {
        room_id: PrimaryKey,
        user_id: PrimaryKey,
        source: String,
    },
    /// A user disconnected from a room
    UserDisconnected {
        room_id: PrimaryKey,
        user_id: PrimaryKey,
        source: String,
    },
}

impl CollabEvent {
    /// Convert a pipeline event to a friendly collab event
    pub fn from_pipeline_event(
        context: &CollabContext,
        event: PipelineEvent,
    ) -> Option<CollabEvent> {
        match event {
            PipelineEvent::PlayerStateUpdate {
                player_id,
                new_state,
            } => context
                .room_by_player_id(player_id)
                .map(|room| Self::PlayerStateUpdate {
                    room_id: room.id(),
                    new_state,
                }),
            PipelineEvent::PlayerTimeUpdate {
                player_id,
                position,
                total_position,
            } => context
                .room_by_player_id(player_id)
                .map(|room| Self::PlayerTimeUpdate {
                    room_id: room.id(),
                    position,
                    total_position,
                }),
            PipelineEvent::PlayerAdvanced { player_id } => context
                .room_by_player_id(player_id)
                .map(|room| Self::RoomQueueItemUpdate {
                    room_id: room.id(),
                    new_item: room.current_item(),
                }),
            _ => None,
        }
    }
}
