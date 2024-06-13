use crate::{
    BufferRead, BufferVoidDistance, Id, MultiRangeBuffer, PipelineContext, PipelineEvent, Sample,
};
use parking_lot::Mutex;

pub type SinkId = Id<Sink>;

/// Represents a source of samples that can be played by a [Player].
pub struct Sink {
    pub id: SinkId,
    context: PipelineContext,
    /// The samples stored in this sink.
    buffer: MultiRangeBuffer,
    /// The expected length of the samples. If this is `None`, the length is unknown.
    expected_length: Option<usize>,
    /// The current load state of the sink.
    load_state: Mutex<SinkLoadState>,
}

/// Represents the load state of a [Sink].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SinkLoadState {
    /// The sink finished loading or hasn't started loading yet.
    #[default]
    Idle,
    /// The [Ingestion] is loading samples into the sink.
    Loading,
    /// The [Ingestion] has finished loading samples into the sink and there is no more data to load.
    ///
    /// If the sink is in this state, it will be skipped by the player when it encounters a void.
    Sealed,
    /// Something went wrong with the sink, or the [Ingestion] loading into it.
    /// Note: This is a string because the error may not be clonable.
    ///
    /// If the sink is in this state, it will be skipped by the player when it encounters a void.
    Error(String),
}

impl Sink {
    pub fn new(context: &PipelineContext, expected_length: Option<usize>) -> Self {
        // If we don't have the length, this is probably a live stream.
        // In that case, allow the buffer to be as big as possible, and allow the [Loadable] to report when the end has been reached instead.
        let buffer_expected_length = expected_length.unwrap_or(usize::MAX);

        Self {
            expected_length,
            context: context.clone(),
            id: SinkId::new(),
            load_state: Default::default(),
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

    pub fn set_load_state(&self, state: SinkLoadState) {
        let mut current_load_state = self.load_state.lock();

        if *current_load_state != state {
            self.context.emit(PipelineEvent::SinkLoadStateUpdate {
                sink_id: self.id,
                new_state: state.clone(),
            });

            *current_load_state = state;
        }
    }

    pub fn load_state(&self) -> SinkLoadState {
        self.load_state.lock().clone()
    }

    /// Returns how many samples are left in the sink until a void at the current offset.
    pub fn distance_from_void(&self, offset: usize) -> BufferVoidDistance {
        self.buffer.distance_from_void(offset)
    }

    /// Returns how many expected samples are left from the given offset.
    pub fn distance_from_end(&self, offset: usize) -> usize {
        self.expected_length
            .unwrap_or(usize::MAX)
            .saturating_sub(offset)
    }

    /// Clears the samples in the sink outside the given window.
    pub fn clear_outside(&self, offset: usize, window: usize, chunk_size: usize) {
        self.buffer.retain_window(offset, window, chunk_size)
    }

    /// Returns the expected length of the sink. [None] if unknown.
    pub fn expected_length(&self) -> Option<usize> {
        self.expected_length
    }

    /// Returns true if the sink can still be loaded into.
    pub fn can_load_more(&self) -> bool {
        matches!(
            self.load_state(),
            SinkLoadState::Loading | SinkLoadState::Idle
        )
    }

    /// Returns true if the sink can be cleared from memory.
    pub fn is_clearable(&self) -> bool {
        false
    }
}

}
