use std::sync::Arc;

use crossbeam::atomic::AtomicCell;

use crate::{
    Id, IdType, Introspect, Output, PipelineAction, PipelineContext, PipelineEvent, Queue, Sink,
    SinkId, Timeline, TimelinePreload,
};

use super::TimelineIntrospection;

pub type PlayerId = Id<Player>;

/// The player is responsible for managing the playback of a [Timeline],
/// and writing the played samples to an output buffer.
pub struct Player {
    pub id: PlayerId,
    context: PipelineContext,
    timeline: Arc<Timeline>,
    output: Arc<Output>,
    state: Arc<AtomicCell<PlayerState>>,
    should_play: AtomicCell<bool>,
}

/// A type used to control a player and read its state.
pub struct PlayerContext {
    pub id: PlayerId,
    context: PipelineContext,
    timeline: Arc<Timeline>,
    state: Arc<AtomicCell<PlayerState>>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PlayerState {
    /// The player is either paused, or has nothing to play.
    /// Note that the player still processes samples even if it is in this state.
    #[default]
    Idle,
    /// The player is playing sinks.
    Playing,
    /// The player is waiting for a sink to load.
    Buffering,
}

impl Player {
    pub fn new(context: &PipelineContext, output: Arc<Output>) -> Self {
        let config = context.config.clone();

        Self {
            timeline: Timeline::new(config.clone()).into(),
            should_play: true.into(),
            context: context.clone(),
            state: Default::default(),
            id: PlayerId::new(),
            output,
        }
    }

    pub fn preload(&self) -> Vec<TimelinePreload> {
        self.timeline.preload()
    }

    pub fn set_sinks(&self, sinks: Vec<Arc<Sink>>) {
        self.timeline.set_sinks(sinks);
    }

    /// Processes the timeline and pushes the samples to the output stream.
    /// If there are no sinks to play, the samples pushed are silence.
    pub fn process(&self) {
        let mut samples = vec![0.; self.context.config.buffer_size_in_samples()];
        let mut amount_read = 0;

        // If the player is not supposed to play, we just push silence.
        if !self.should_play.load() {
            self.output.push(self.id, samples);
            self.set_state_if_different(PlayerState::Idle);

            return;
        }

        // Get the current sink before advancing the timeline.
        let current_sink = self.timeline.current_sink();
        let reads = self.timeline.advance(samples.len());

        // If this new sink is different, we can be sure that we advanced to the next sink.
        let new_sink = self.timeline.current_sink();
        let was_empty = reads.is_empty();

        // Update state according to the result of the reads.
        if was_empty {
            // If the timeline is empty, we reached the end of the playback.
            if self.timeline.is_empty() {
                self.set_state_if_different(PlayerState::Idle);
            } else {
                // Otherwise, a sink is buffering at this point.
                self.set_state_if_different(PlayerState::Buffering);
            }
        } else {
            self.set_state_if_different(PlayerState::Playing);
        }

        for read in reads {
            let slice = &mut samples[amount_read..];

            let sink = self
                .context
                .sinks
                .get(&read.sink_id)
                .expect("Sink exists when trying to read from it");

            let result = sink.read(read.offset, slice);
            amount_read += result.amount;
        }

        if new_sink != current_sink && current_sink.is_some() {
            self.advance_queue_if_exists()
        }

        // Emit the current time and total time.
        if !was_empty {
            let current_time = self.timeline.current_offset();
            let current_total_time = self.timeline.total_offset();

            self.context.emit(PipelineEvent::PlayerTimeUpdate {
                player_id: self.id,
                position: self.context.config.samples_to_seconds(current_time),
                total_position: self.context.config.samples_to_seconds(current_total_time),
            })
        }

        self.output.push(self.id, samples);
    }

    /// Clears samples that are not needed, to save memory.
    pub fn clear_superflous(&self) {
        self.timeline.clear_superflous();
    }

    /// Starts playback if possible.
    pub fn play(&self) {
        self.should_play.store(true);
    }

    /// Pauses playback.
    pub fn pause(&self) {
        self.should_play.store(false);
    }

    /// Seeks to a specific offset.
    pub fn seek(&self, offset: usize) {
        self.timeline.seek(offset);
    }

    /// Returns the context for this player.
    pub fn context(&self) -> PlayerContext {
        PlayerContext {
            id: self.id,
            state: self.state.clone(),
            context: self.context.clone(),
            timeline: self.timeline.clone(),
        }
    }

    fn set_state_if_different(&self, state: PlayerState) {
        if self.state.load() != state {
            self.context.emit(PipelineEvent::PlayerStateUpdate {
                player_id: self.id,
                new_state: state,
            });

            self.state.store(state);
        }
    }

    /// Advances the queue associated with this player if it exists.
    fn advance_queue_if_exists(&self) {
        let queue = self.context.queues.get(&self.id);

        if let Some(queue) = queue {
            // If the queue is implemented correctly, this should notify the queue system to update the sinks in this player.
            queue.next();

            // Emit an event to notify that the player has advanced.
            self.context
                .emit(PipelineEvent::PlayerAdvanced { player_id: self.id });
        }
    }
}

impl PlayerContext {
    /// Starts playback if possible.
    pub fn play(&self) {
        self.context
            .dispatch(PipelineAction::PlayPlayer { player_id: self.id });
    }

    /// Pauses playback.
    pub fn pause(&self) {
        self.context
            .dispatch(PipelineAction::PausePlayer { player_id: self.id });
    }

    /// Seeks to a specific time.
    /// * `position` is the time in seconds.
    pub fn seek(&self, position: f32) {
        self.context.dispatch(PipelineAction::SeekPlayer {
            player_id: self.id,
            position,
        });
    }

    /// Returns the current position in seconds.
    pub fn current_time(&self) -> f32 {
        self.context
            .config
            .samples_to_seconds(self.timeline.current_offset())
    }

    /// Returns the current total position in seconds.
    /// This is how long the player has been playing for.
    pub fn current_total_time(&self) -> f32 {
        self.context
            .config
            .samples_to_seconds(self.timeline.total_offset())
    }

    /// Returns the currently playing sink.
    pub fn current_sink(&self) -> Option<SinkId> {
        self.timeline.current_sink()
    }

    /// Returns the current state of the player
    pub fn current_state(&self) -> PlayerState {
        self.state.load()
    }
}

#[derive(Debug)]
pub struct PlayerIntrospection {
    pub id: IdType,
    pub state: PlayerState,
    pub should_play: bool,
    pub timeline: TimelineIntrospection,
}

impl Introspect<PlayerIntrospection> for Player {
    fn introspect(&self) -> PlayerIntrospection {
        PlayerIntrospection {
            id: self.id.value(),
            state: self.state.load(),
            should_play: self.should_play.load(),
            timeline: self.timeline.introspect(),
        }
    }
}
