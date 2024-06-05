use std::default;

use crate::{Id, MultiRangeBuffer};
use parking_lot::Mutex;

pub type SinkId = Id<Sink>;

/// Represents a source of samples that can be played.
///
/// This is what a [Loader] will load into.
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
#[derive(Debug, Clone, Default)]
pub enum SinkState {
    /// Nothing is happening with the sink right now.
    /// It may still be read from during this state.
    #[default]
    Idle,
    /// A loader is loading [Sample] into the sink.
    Loading,
    /// The sink will not receive any more samples.
    Sealed,
    /// The sink has been consumed and is ready to be dropped from memory.
    Consumed,
    /// Something went wrong with the sink, or the [Loader] loading into it.
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

    pub fn set_state(&self, state: SinkState) {
        *self.state.lock() = state;
    }

    pub fn state(&self) -> SinkState {
        self.state.lock().clone()
    }

    /// Writes samples to the sink at the given offset.
    pub fn write(&self, offset: usize, samples: &[f32]) {
        self.buffer.write(offset, samples);
    }
}
