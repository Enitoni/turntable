use std::{sync::Arc, time::Instant};

use crate::{
    BufferRead, BufferVoidDistance, Id, IdType, Introspect, MultiRangeBuffer,
    MultiRangeBufferIntrospection, PipelineContext, PipelineEvent, Sample,
};
use crossbeam::atomic::AtomicCell;
use log::info;
use parking_lot::{Mutex, RwLock};

pub type SinkId = Id<Sink>;

/// Represents a source of samples that can be played by a [Player].
pub struct Sink {
    pub id: SinkId,
    context: PipelineContext,
    activation: RwLock<SinkActivation>,
    /// The current load state of the sink.
    load_state: Mutex<SinkLoadState>,
    /// Whether a guard has been created and exists somewhere.
    has_guard: AtomicCell<bool>,
    /// Whether an activation guard has been created and exists somewhere.
    has_activation_guard: AtomicCell<bool>,
    /// Whether a write reference has been created and exists somewhere.
    has_write_ref: AtomicCell<bool>,
    /// The time since the sink was last interacted with.
    duration_since_interaction: AtomicCell<Instant>,
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

/// Represents the activation state of a sink
#[derive(Debug, Default)]
pub enum SinkActivation {
    /// The sink has been prepared
    #[default]
    Inactive,
    /// The sink is currently being activated
    Activating,
    /// The sink has been successfully activated and is ready to be loaded to
    Activated(MultiRangeBuffer),
    /// An error occurred with sink activation and the sink will be skipped
    Error(String),
}

/// A reference to a sink that determines if it is used by a [Timeline].
/// It is held by a [Timeline] and when dropped, the sink can be cleared from memory.
pub struct SinkGuard {
    pub id: SinkId,
    context: PipelineContext,
}

/// A reference to a sink that can be written to.
/// It is used by [Ingestion] and when dropped, the sink can be cleared from memory.
pub struct WriteGuard {
    id: SinkId,
    context: PipelineContext,
}

/// A reference to a sink that allows activation.
/// It is used to either finish an activation or fail it. When dropped, the sink can be cleared from memory.
pub struct ActivationGuard {
    id: SinkId,
    context: PipelineContext,
    finished: AtomicCell<bool>,
}

impl Sink {
    /// Creates a new inactive sink
    pub fn prepare(context: &PipelineContext) -> Self {
        Self {
            id: SinkId::new(),
            context: context.clone(),
            has_guard: Default::default(),
            load_state: Default::default(),
            activation: Default::default(),
            has_write_ref: Default::default(),
            has_activation_guard: Default::default(),
            duration_since_interaction: Instant::now().into(),
        }
    }

    /// Creates a new sink that is activated immediately
    pub fn with_activation(context: &PipelineContext, expected_length: Option<usize>) -> Self {
        let me = Self::prepare(context);

        *me.activation.write() = SinkActivation::Activated(MultiRangeBuffer::new(expected_length));
        me
    }

    pub fn guard(&self) -> SinkGuard {
        assert!(!self.has_guard.load(), "Sink already has a guard");

        self.has_guard.store(true);
        self.interact();

        SinkGuard {
            context: self.context.clone(),
            id: self.id,
        }
    }

    /// Prepares activation by returning an activation guard.
    pub fn activate(&self) -> ActivationGuard {
        let mut activation = self.activation.write();

        assert!(
            matches!(
                *activation,
                SinkActivation::Inactive | SinkActivation::Error(_)
            ),
            "Sink is inactive when trying to activate"
        );

        assert!(
            !self.has_activation_guard.load(),
            "Sink already has activation guard"
        );

        info!("Activating sink #{}", self.id);

        self.has_activation_guard.store(true);
        self.interact();

        *activation = SinkActivation::Activating;

        ActivationGuard {
            id: self.id,
            context: self.context.clone(),
            finished: false.into(),
        }
    }

    /// Reads samples from the sink at the given offset.
    pub fn read(&self, offset: usize, buf: &mut [Sample]) -> BufferRead {
        self.read_buffer(|buffer| buffer.read(offset, buf))
    }

    /// Returns a write reference to the sink.
    /// Only one write reference can exist at a time.
    pub fn write(&self) -> WriteGuard {
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
        self.interact();

        WriteGuard {
            context: self.context.clone(),
            id: self.id,
        }
    }

    pub fn load_state(&self) -> SinkLoadState {
        self.load_state.lock().clone()
    }

    /// Returns how many samples are left in the sink until a void at the current offset.
    fn distance_from_void(&self, offset: usize) -> BufferVoidDistance {
        self.read_buffer(|buffer| buffer.distance_from_void(offset))
    }

    /// Returns how many expected samples are left from the given offset.
    fn distance_from_end(&self, offset: usize) -> usize {
        self.read_buffer(|buffer| buffer.distance_from_end(offset))
    }

    /// Clears the samples in the sink outside the given window.
    fn clear_outside(&self, offset: usize, window: usize, chunk_size: usize) {
        self.write_buffer(|buffer| buffer.retain_window(offset, window, chunk_size));
    }

    /// Returns the expected length of the sink. [None] if unknown.
    pub fn expected_length(&self) -> Option<usize> {
        self.read_buffer(|buffer| buffer.expected_length())
    }

    /// Returns true if the sink can still be loaded into.
    fn can_load_more(&self) -> bool {
        matches!(
            self.load_state(),
            SinkLoadState::Loading | SinkLoadState::Idle
        )
    }

    /// Returns true if the sink is inactive
    pub fn is_activatable(&self) -> bool {
        matches!(*self.activation.read(), SinkActivation::Inactive)
            && !self.has_activation_guard.load()
    }

    /// Returns true if the sink is activated and can be read from
    pub fn is_activated(&self) -> bool {
        matches!(*self.activation.read(), SinkActivation::Activated(_))
    }

    /// Returns true if the sink can be cleared from memory.
    pub fn is_clearable(&self) -> bool {
        let has_read_ref = self.has_guard.load();
        let has_write_ref = self.has_write_ref.load();

        let elapsed_secs = self.duration_since_interaction.load().elapsed().as_secs();

        // One minute should be enough to clear sinks before too much memory is used.
        if elapsed_secs < 60 {
            return false;
        }

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
        self.has_guard.store(false);
        self.interact();
    }

    fn clear_activation_guard(&self) {
        self.has_activation_guard.store(false);
        self.interact();
    }

    fn clear_write_ref(&self) {
        // Reset load state to Idle if it is still loading.
        if self.load_state() == SinkLoadState::Loading {
            self.set_load_state(SinkLoadState::Idle);
        }

        self.has_write_ref.store(false);
        self.interact();
    }

    /// Writes samples to the sink at the given offset.
    fn internal_write(&self, offset: usize, samples: &[Sample]) {
        self.write_buffer(|buffer| {
            buffer.write(offset, samples);
        });

        info!(
            "Wrote {} samples at offset {} into sink #{}",
            samples.len(),
            offset,
            self.id
        );
    }

    fn interact(&self) {
        self.duration_since_interaction.store(Instant::now());
    }

    fn read_buffer<F, O>(&self, cb: F) -> O
    where
        F: FnOnce(&MultiRangeBuffer) -> O,
    {
        let activation = self.activation.read();

        if let SinkActivation::Activated(buffer) = &*activation {
            cb(buffer)
        } else {
            panic!("Cannot read buffer of sink that is not activated")
        }
    }

    fn write_buffer<F, O>(&self, cb: F) -> O
    where
        F: FnOnce(&mut MultiRangeBuffer) -> O,
    {
        let mut activation = self.activation.write();

        if let SinkActivation::Activated(buffer) = &mut *activation {
            cb(buffer)
        } else {
            panic!("Cannot write buffer of sink that is not activated")
        }
    }
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

    pub fn is_activated(&self) -> bool {
        self.get_sink().is_activated()
    }

    pub fn expected_length(&self) -> Option<usize> {
        self.get_sink().expected_length()
    }
}

impl WriteGuard {
    fn get_sink(&self) -> Arc<Sink> {
        self.context
            .sinks
            .get(&self.id)
            .expect("WriteGuard has associated Sink")
            .clone()
    }

    pub fn write(&self, offset: usize, samples: &[Sample]) {
        self.get_sink().internal_write(offset, samples);
    }

    /// Seals the sink.
    pub fn seal(&self) {
        self.get_sink().set_load_state(SinkLoadState::Sealed);
    }

    /// Sets the sink to the given error state.
    pub fn error(&self, error: String) {
        self.get_sink().set_load_state(SinkLoadState::Error(error));
    }
}

impl ActivationGuard {
    fn get_sink(&self) -> Arc<Sink> {
        self.context
            .sinks
            .get(&self.id)
            .expect("SinkGuard has associated Sink")
            .clone()
    }

    pub fn activate(self, expected_length: Option<usize>) {
        self.finished.store(true);

        info!(
            "Sink #{} activated with length: {:?}",
            self.id, expected_length
        );

        *self.get_sink().activation.write() =
            SinkActivation::Activated(MultiRangeBuffer::new(expected_length));
    }

    pub fn fail(self, reason: &str) {
        self.finished.store(true);

        info!("Sink #{} failed to activate: {}", self.id, reason);

        *self.get_sink().activation.write() = SinkActivation::Error(reason.to_string());
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

impl Drop for WriteGuard {
    fn drop(&mut self) {
        let sink = self
            .context
            .sinks
            .get(&self.id)
            .expect("SinkWriteRef about to be dropped has associated Sink");

        sink.clear_write_ref();
    }
}

impl Drop for ActivationGuard {
    fn drop(&mut self) {
        assert!(
            self.finished.load(),
            "Activation guard was dropped before being finished"
        );

        let sink = self
            .context
            .sinks
            .get(&self.id)
            .expect("SinkGuard about to be dropped has associated Sink");

        sink.clear_activation_guard();
    }
}

/// See [SinkActivation].
#[derive(Debug)]
pub enum ActivationIntrospection {
    Inactive,
    Activating,
    Activated {
        buffer: MultiRangeBufferIntrospection,
    },
    Error {
        reason: String,
    },
}

/// See [SinkLoadState].
#[derive(Debug)]
pub enum LoadStateIntrospection {
    Idle,
    Loading,
    Sealed,
    Error { reason: String },
}

#[derive(Debug)]
pub struct SinkIntrospection {
    pub id: IdType,
    pub activation_state: ActivationIntrospection,
    pub load_state: LoadStateIntrospection,
    /// The time since the sink was last interacted with, in seconds
    pub duration_since_interaction: f32,
    pub is_clearable: bool,
}

impl Introspect<ActivationIntrospection> for SinkActivation {
    fn introspect(&self) -> ActivationIntrospection {
        match self {
            SinkActivation::Inactive => ActivationIntrospection::Inactive,
            SinkActivation::Activating => ActivationIntrospection::Activating,
            SinkActivation::Activated(buffer) => ActivationIntrospection::Activated {
                buffer: buffer.introspect(),
            },
            SinkActivation::Error(err) => ActivationIntrospection::Error {
                reason: err.clone(),
            },
        }
    }
}

impl Introspect<LoadStateIntrospection> for SinkLoadState {
    fn introspect(&self) -> LoadStateIntrospection {
        match self {
            SinkLoadState::Idle => LoadStateIntrospection::Idle,
            SinkLoadState::Loading => LoadStateIntrospection::Loading,
            SinkLoadState::Sealed => LoadStateIntrospection::Sealed,
            SinkLoadState::Error(err) => LoadStateIntrospection::Error {
                reason: err.clone(),
            },
        }
    }
}

impl Introspect<SinkIntrospection> for Sink {
    fn introspect(&self) -> SinkIntrospection {
        SinkIntrospection {
            id: self.id.value(),
            activation_state: self.activation.read().introspect(),
            load_state: self.load_state.lock().introspect(),
            is_clearable: self.is_clearable(),
            duration_since_interaction: self
                .duration_since_interaction
                .load()
                .elapsed()
                .as_secs_f32(),
        }
    }
}

impl SinkIntrospection {
    pub fn size(&self) -> usize {
        if let ActivationIntrospection::Activated { buffer } = &self.activation_state {
            buffer.current_size
        } else {
            0
        }
    }
}
