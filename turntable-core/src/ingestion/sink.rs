use std::sync::Arc;

use crate::{
    BufferRead, BufferVoidDistance, Id, MultiRangeBuffer, PipelineContext, PipelineEvent, Sample,
};
use crossbeam::atomic::AtomicCell;
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

    /// Whether a guard has been created and exists somewhere.
    has_guard: AtomicCell<bool>,
    /// Whether a write reference has been created and exists somewhere.
    has_write_ref: AtomicCell<bool>,
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
            id: SinkId::new(),
            expected_length,
            context: context.clone(),
            load_state: Default::default(),
            has_guard: Default::default(),
            has_write_ref: Default::default(),
            buffer: MultiRangeBuffer::new(buffer_expected_length),
        }
    }

    pub fn guard(&self) -> SinkGuard {
        assert!(!self.has_guard.load(), "Sink already has a guard");
        self.has_guard.store(true);

        SinkGuard {
            context: self.context.clone(),
            id: self.id,
        }
    }

    /// Reads samples from the sink at the given offset.
    pub fn read(&self, offset: usize, buf: &mut [Sample]) -> BufferRead {
        self.buffer.read(offset, buf)
    }

    /// Returns a write reference to the sink.
    /// Only one write reference can exist at a time.
    pub fn write(&self) -> SinkWriteRef {
        assert!(
            !self.has_write_ref.load(),
            "Sink already has a write reference"
        );

        assert_eq!(
            self.load_state(),
            SinkLoadState::Idle,
            "Sink must be idle to write"
        );

        self.has_write_ref.store(true);
        self.set_load_state(SinkLoadState::Loading);

        SinkWriteRef {
            context: &self.context,
            id: self.id,
        }
    }

    /// Seals the sink.
    pub fn seal(&self) {
        self.set_load_state(SinkLoadState::Sealed);
    }

    /// Sets the sink to the given error state.
    pub fn error(&self, error: String) {
        self.set_load_state(SinkLoadState::Error(error));
    }

    pub fn load_state(&self) -> SinkLoadState {
        self.load_state.lock().clone()
    }

    /// Returns how many samples are left in the sink until a void at the current offset.
    fn distance_from_void(&self, offset: usize) -> BufferVoidDistance {
        self.buffer.distance_from_void(offset)
    }

    /// Returns how many expected samples are left from the given offset.
    fn distance_from_end(&self, offset: usize) -> usize {
        self.expected_length
            .unwrap_or(usize::MAX)
            .saturating_sub(offset)
    }

    /// Clears the samples in the sink outside the given window.
    fn clear_outside(&self, offset: usize, window: usize, chunk_size: usize) {
        self.buffer.retain_window(offset, window, chunk_size)
    }

    /// Returns the expected length of the sink. [None] if unknown.
    pub fn expected_length(&self) -> Option<usize> {
        self.expected_length
    }

    /// Returns true if the sink can still be loaded into.
    fn can_load_more(&self) -> bool {
        matches!(
            self.load_state(),
            SinkLoadState::Loading | SinkLoadState::Idle
        )
    }

    /// Returns true if the sink can be cleared from memory.
    pub fn is_clearable(&self) -> bool {
        let has_read_ref = self.has_guard.load();
        let has_write_ref = self.has_write_ref.load();

        !has_read_ref && !has_write_ref
    }

    fn set_load_state(&self, state: SinkLoadState) {
        let mut current_load_state = self.load_state.lock();

        if *current_load_state != state {
            self.context.emit(PipelineEvent::SinkLoadStateUpdate {
                sink_id: self.id,
                new_state: state.clone(),
            });

            *current_load_state = state;
        }
    }

    fn clear_guard(&self) {
        self.has_guard.store(false)
    }

    fn clear_write_ref(&self) {
        // Reset load state to Idle if it is still loading.
        if self.load_state() == SinkLoadState::Loading {
            self.set_load_state(SinkLoadState::Idle);
        }

        self.has_write_ref.store(false)
    }

    /// Writes samples to the sink at the given offset.
    fn internal_write(&self, offset: usize, samples: &[Sample]) {
        self.buffer.write(offset, samples);
    }
}

/// A reference to a sink that determines if it is used by a [Timeline].
/// It is held by a [Timeline] and when dropped, the sink can be cleared from memory.
pub struct SinkGuard {
    context: PipelineContext,
    pub id: SinkId,
}

/// A reference to a sink that can be written to.
/// It is used by [Ingestion] and when dropped, the sink can be cleared from memory.
pub struct SinkWriteRef<'write> {
    context: &'write PipelineContext,
    id: SinkId,
}

impl SinkGuard {
    fn get_sink(&self) -> Arc<Sink> {
        self.context
            .sinks
            .get(&self.id)
            .expect("SinkGuard has associated Sink")
            .clone()
    }

    /// Returns how many samples are left in the sink until a void at the current offset.
    pub fn distance_from_void(&self, offset: usize) -> BufferVoidDistance {
        self.get_sink().distance_from_void(offset)
    }

    /// Returns how many expected samples are left from the given offset.
    pub fn distance_from_end(&self, offset: usize) -> usize {
        self.get_sink().distance_from_end(offset)
    }

    /// Returns true if the sink can still be loaded into.
    pub fn can_load_more(&self) -> bool {
        self.get_sink().can_load_more()
    }

    pub fn clear_outside(&self, offset: usize, window: usize, chunk_size: usize) {
        self.get_sink().clear_outside(offset, window, chunk_size);
    }
}

impl SinkWriteRef<'_> {
    pub fn write(&self, offset: usize, samples: &[Sample]) {
        let sink = self
            .context
            .sinks
            .get(&self.id)
            .expect("SinkWriteRef has associated Sink");

        sink.internal_write(offset, samples);
    }
}

impl Drop for SinkGuard {
    fn drop(&mut self) {
        let sink = self
            .context
            .sinks
            .get(&self.id)
            .expect("SinkGuard about to be dropped has associated Sink");

        sink.clear_guard();
    }
}

impl Drop for SinkWriteRef<'_> {
    fn drop(&mut self) {
        let sink = self
            .context
            .sinks
            .get(&self.id)
            .expect("SinkWriteRef about to be dropped has associated Sink");

        sink.clear_write_ref();
    }
}
