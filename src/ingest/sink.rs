use crate::{
    audio::{util::Buffer, Sample},
    store::Id,
    util::{sync::Wait},
};
use crossbeam::atomic::AtomicCell;
use std::{fmt::Display, sync::Arc};

pub type SinkId = Id<InternalSink>;
pub type Sink = Arc<InternalSink>;

/// A destination sink, serving as a source of samples.
/// This is what is written to when loading an [Input].
#[derive(Debug)]
pub struct InternalSink {
    id: SinkId,
    samples: Buffer,
    expected_length: SinkLength,

    status: AtomicCell<SinkStatus>,
    wait: Wait,

    /// This is true when the sink is pending (being loaded into)
    pub(super) pending: AtomicCell<bool>,

    /// This is true when the sink should be cleared and deleted
    pub(super) consumed: AtomicCell<bool>,
}

/// A length in [Sample]
#[derive(Debug, Default, Clone, Copy)]
pub enum SinkLength {
    /// We know the exact length. This is the best case scenario.
    Exact(usize),
    /// We don't have the exact length, but have approximated it based on things like duration and file size.
    Approximate(usize),
    /// The length is unknown or unavailable. This is typically used for livestreams.
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy)]
pub enum SinkStatus {
    /// The sink has received all the samples available, with the final amount of samples.
    /// Note that this may not be the same as the expected length.
    Completed(usize),
    /// The sink has only received this amount of samples.
    Partial(usize),
    /// An error occurred, and the sink should be skipped.
    Error,
}

impl InternalSink {
    pub fn new(length: SinkLength) -> Self {
        let buffer_size = length.value();

        Self {
            id: SinkId::new(),
            samples: Buffer::new(buffer_size),
            status: SinkStatus::Partial(0).into(),
            expected_length: length,
            consumed: false.into(),
            pending: false.into(),
            wait: Wait::default(),
        }
    }

    pub fn write(&self, samples: &[Sample]) {
        self.samples.write_at_end(samples);
        self.status
            .store(SinkStatus::Partial(self.samples.length()));

        self.pending.store(false);
    }

    pub fn read(&self, offset: usize, buf: &mut [Sample]) -> usize {
        self.samples.read(offset, buf)
    }

    pub fn seal(&self) {
        self.status
            .store(SinkStatus::Completed(self.samples.length()));

        self.wait.notify();
    }

    pub fn id(&self) -> SinkId {
        self.id
    }

    pub fn available(&self) -> usize {
        self.status.load().amount()
    }

    pub fn length(&self) -> SinkLength {
        self.expected_length
    }

    pub fn remaining(&self) -> usize {
        self.expected_length
            .value()
            .saturating_sub(self.available())
            .max(0)
    }

    pub fn expected(&self) -> usize {
        self.expected_length.value()
    }

    pub fn is_complete(&self) -> bool {
        matches!(
            self.status.load(),
            SinkStatus::Completed(_) | SinkStatus::Error
        )
    }

    pub fn is_consumed(&self) -> bool {
        self.consumed.load()
    }

    pub fn is_pending(&self) -> bool {
        self.pending.load()
    }

    pub fn consume(&self) {
        self.consumed.store(true)
    }

    pub(super) fn clear(&self) {
        if !self.consumed.load() {
            panic!("Attempt to clear sink before consumption")
        }

        self.samples.clear();
    }

    pub fn wait_for_write(&self) {
        self.wait.wait()
    }
}

impl SinkLength {
    fn value(&self) -> usize {
        match self {
            Self::Unknown => usize::MAX,
            Self::Exact(x) | Self::Approximate(x) => *x,
        }
    }
}

impl Display for SinkLength {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SinkLength::Exact(x) => write!(f, "{}", x),
            SinkLength::Approximate(x) => write!(f, "~{}", x),
            SinkLength::Unknown => write!(f, "?"),
        }
    }
}

impl SinkStatus {
    fn amount(&self) -> usize {
        match self {
            Self::Completed(x) | Self::Partial(x) => *x,
            SinkStatus::Error => 0,
        }
    }
}
