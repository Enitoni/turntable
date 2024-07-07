use crossbeam::channel::{Receiver, Sender};
use log::{error, info, trace};

use crate::{PlayerId, PlayerState, SinkId, SinkLoadState};

pub type EventSender = Sender<PipelineEvent>;
pub type EventReceiver = Receiver<PipelineEvent>;

pub type ActionSender = Sender<PipelineAction>;
pub type ActionReceiver = Receiver<PipelineAction>;

/// Describes the events that can be emitted by the pipeline.
#[derive(Debug)]
pub enum PipelineEvent {
    /// A sink's state has changed.
    SinkLoadStateUpdate {
        sink_id: SinkId,
        new_state: SinkLoadState,
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
    /// A player advanced to the next queue item.
    PlayerAdvanced { player_id: PlayerId },
    /// A queue item has been ingested
    QueueItemActivated {
        /// The id of the player the queue item's queue belongs to.
        player_id: PlayerId,
        /// The id of the new sink created for the queue item.
        new_sink_id: SinkId,
        /// The custom identifier of the queue item.
        item_id: String,
    },
    /// A queue item failed to be ingested.
    QueueItemActivationError {
        /// The id of the player the queue item's queue belongs to.
        player_id: PlayerId,
        /// The custom identifier of the queue item.
        item_id: String,
        /// The error that happened while activating the queue item.
        error: String,
    },
}

/// Describes an action to be performed on the pipeline.
#[derive(Debug)]
pub enum PipelineAction {
    /// The pipeline should notify a player of a queue update.
    NotifyQueueUpdate {
        // The player the queue belongs to.
        player_id: PlayerId,
    },
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

impl PipelineEvent {
    pub fn log(&self) {
        match self {
            PipelineEvent::SinkLoadStateUpdate { sink_id, new_state } => {
                info!("Sink #{} load state updated: {:?}", sink_id, new_state,);
            }
            PipelineEvent::PlayerStateUpdate {
                player_id,
                new_state,
            } => {
                info!("Player #{} state update: {:?}", player_id, new_state)
            }
            PipelineEvent::PlayerTimeUpdate {
                player_id,
                position,
                total_position: _,
            } => {
                trace!("Player #{} time update: {}", player_id, position)
            }
            PipelineEvent::PlayerAdvanced { player_id } => {
                info!("Player #{} advanced", player_id)
            }
            PipelineEvent::QueueItemActivated {
                player_id,
                new_sink_id,
                item_id,
            } => info!(
                "Queue item {} of player #{} activated with sink #{}",
                item_id, player_id, new_sink_id
            ),
            PipelineEvent::QueueItemActivationError {
                player_id,
                item_id,
                error,
            } => error!(
                "Queue item {} of player #{} failed to activate: {}",
                item_id, player_id, error
            ),
        }
    }
}
