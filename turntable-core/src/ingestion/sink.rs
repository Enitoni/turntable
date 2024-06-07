use crate::{BufferRead, BufferVoidDistance, Id, MultiRangeBuffer, Sample};
use parking_lot::Mutex;

pub type SinkId = Id<Sink>;

/// Represents a source of samples that can be played by a [Player].
#[derive(Debug)]
pub struct Sink {
    pub id: SinkId,
    /// The samples stored in this sink.
    buffer: MultiRangeBuffer,
    /// The expected length of the samples. If this is `None`, the length is unknown.
    expected_length: Option<usize>,
    /// The current state of the sink.
    state: Mutex<SinkState>,
}

/// Represents the lifecycle of a [Sink].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SinkState {
    /// Nothing is happening with the sink right now.
    /// It may still be read from during this state.
    #[default]
    Idle,
    /// The sink is idle, but in use by a [Player].
    Active,
    /// The [Ingestion] is loading samples into the sink.
    Loading,
    /// The sink will not receive any more samples.
    Sealed,
    /// Something went wrong with the sink, or the [Ingestion] loading into it.
    /// If the sink is in this state, it will be skipped by the player.
    ///
    /// Note: This is a string because the error may not be clonable.
    Error(String),
}

impl Sink {
    pub fn new(expected_length: Option<usize>) -> Self {
        // If we don't have the length, this is probably a live stream.
        // In that case, allow the buffer to be as big as possible, and allow the [Loadable] to report when the end has been reached instead.
        let buffer_expected_length = expected_length.unwrap_or(usize::MAX);

        Self {
            expected_length,
            id: SinkId::new(),
            state: Default::default(),
            buffer: MultiRangeBuffer::new(buffer_expected_length),
        }
    }

    pub fn read(&self, offset: usize, buf: &mut [Sample]) -> BufferRead {
        self.buffer.read(offset, buf)
    }

    /// Writes samples to the sink at the given offset.
    pub fn write(&self, offset: usize, samples: &[Sample]) {
        self.buffer.write(offset, samples);
    }

    pub fn set_state(&self, state: SinkState) {
        *self.state.lock() = state;
    }

    pub fn state(&self) -> SinkState {
        self.state.lock().clone()
    }

    /// Returns how many samples are left in the sink until a void at the current offset.
    pub fn distance_from_void(&self, offset: usize) -> BufferVoidDistance {
        self.buffer.distance_from_void(offset)
    }

    /// Clears the samples in the sink outside the given window.
    pub fn clear_outside(&self, offset: usize, window: usize) {
        self.buffer.retain_window(offset, window)
    }

    /// Marks the sink as idle, meaning it can be dropped from memory.
    pub fn deactivate(&self) {
        self.set_state(SinkState::Idle);
    }

    /// Marks the sink as active, meaning it is in use and cannot be dropped.
    pub fn activate(&self) {
        self.set_state(SinkState::Active);
    }

    /// Returns true if the sink is idle, loading, or sealed.
    /// That means it can be played by a [Player].
    pub fn is_playable(&self) -> bool {
        matches!(
            self.state(),
            SinkState::Active | SinkState::Loading | SinkState::Sealed
        )
    }

    /// Returns true if the sink can still be loaded into.
    /// If this is false, the sink should be advanced past once the last loaded samples have been played.
    pub fn is_loadable(&self) -> bool {
        !matches!(
            self.state(),
            SinkState::Error(_) | SinkState::Sealed | SinkState::Active
        )
    }
}
