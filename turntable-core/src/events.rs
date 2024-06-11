use crossbeam::channel::{Receiver, Sender};

use crate::{PlayerId, PlayerState, SinkId, SinkState};

pub type EventSender = Sender<PipelineEvent>;
pub type EventReceiver = Receiver<PipelineEvent>;

pub type ActionSender = Sender<PipelineAction>;
pub type ActionReceiver = Receiver<PipelineAction>;

/// Describes the events that can be emitted by the pipeline.
#[derive(Debug)]
pub enum PipelineEvent {
    /// A sink's state has changed.
    SinkStateUpdate {
        sink_id: SinkId,
        new_state: SinkState,
    },
    /// A player's state has changed.
    PlayerStateUpdate {
        player_id: PlayerId,
        new_state: PlayerState,
    },
    /// A player's playback offset has changed.
    PlayerTimeUpdate {
        player_id: PlayerId,
        /// The current position of the player, in seconds.
        position: f32,
        /// The total position of the player, in seconds.
        total_position: f32,
    },
}

/// Describes an action to be performed on the pipeline.
#[derive(Debug)]
pub enum PipelineAction {
    /// The player of the given id should resume or start playing.
    PlayPlayer { player_id: PlayerId },
    /// The player of the given id should pause.
    PausePlayer { player_id: PlayerId },
    /// The player of the given id should seek to the given position.
    SeekPlayer {
        player_id: PlayerId,
        /// The position to seek to, in seconds.
        position: f32,
    },
}
