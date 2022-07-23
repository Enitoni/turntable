use colored::Colorize;
use crossbeam::atomic::AtomicCell;
use log::trace;

use super::{Sample, SAMPLES_PER_SEC};
use crate::{
    audio::{
        pipeline::{SampleReader, SampleSource, SamplesRead},
        util::Buffer,
    },
    logging::LogColor,
    util::model::{Id, Identified, Store},
};
use std::sync::{Arc, Mutex};

pub type LoaderId = Id<Loader>;

/// Loads an audio source into memory while caching on disk.
/// It will read from disk if the source is already cached.
pub struct Loader {
    id: LoaderId,
    buffer: Buffer,
    source: Mutex<SampleSource>,
    length: AtomicCell<usize>,
}

impl Identified for Loader {
    const NAME: &'static str = "Loader";

    fn id(&self) -> Id<Self> {
        self.id
    }
}

impl Loader {
    pub fn load(&self, amount: usize) -> SamplesRead {
        let mut source = self.source.lock().unwrap();

        trace!(
            "{}: {}",
            self.id,
            format!("Loading {} samples", amount).color(LogColor::White),
        );

        let (result, buf) = source.read_samples_to_vec(amount);
        self.buffer.write_at_end(&buf[..result.amount()]);

        dbg!(&result);

        // The data might have ended earlier than expected,
        // so we change the size to ensure correctness.
        if let SamplesRead::Empty(_) = result {
            self.length.store(self.buffer.length());

            trace!(
                "{}: {}",
                self.id,
                format!("Ended earlier than expected at {} samples", self.expected())
                    .color(LogColor::Orange),
            );
        }

        trace!(
            "{}: {}",
            self.id,
            format!("Received {}/{} samples", self.available(), self.expected())
                .color(LogColor::Success),
        );

        result
    }

    pub fn read(&self, offset: usize, buf: &mut [Sample]) -> usize {
        self.buffer.read(offset, buf)
    }

    pub fn available(&self) -> usize {
        self.buffer.length()
    }

    pub fn expected(&self) -> usize {
        self.length.load()
    }
}

/// Manages all loaders
pub struct Pool {
    store: Store<Loader>,
}

impl Pool {
    pub fn new() -> Self {
        Self {
            store: Store::new(),
        }
    }

    pub fn add<R: 'static + SampleReader + Send + Sync>(
        &self,
        reader: R,
        length: usize,
    ) -> Arc<Loader> {
        let loader = Loader {
            id: LoaderId::new(),
            buffer: Buffer::new(length),
            source: Mutex::new(reader.wrap()),
            length: length.into(),
        };

        let id = self.store.insert(loader);
        self.store.get_expect(id)
    }

    pub fn load(&self, id: LoaderId, amount: usize) -> usize {
        let loader = self.store.get_expect(id);

        loader.load(amount);
        loader.available()
    }

    pub fn read(&self, id: LoaderId, offset: usize, buf: &mut [Sample]) -> usize {
        self.store.get_expect(id).read(offset, buf)
    }
}

/// How many samples to load after hitting the threshold.
pub const PRELOAD_AMOUNT: usize = SAMPLES_PER_SEC * 60;

/// The threshold at which loading more samples happens
pub const PRELOAD_THRESHOLD: usize = SAMPLES_PER_SEC * 20;
