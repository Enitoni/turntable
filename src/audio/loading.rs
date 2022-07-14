use std::{
    hash::Hasher,
    ops::Range,
    sync::{
        mpsc::{sync_channel, SyncSender},
        Arc, Mutex,
    },
    thread,
};

use super::{queuing::QueueEvent, source::Error as SourceError, AudioEvent, SAMPLES_PER_SEC};
use crate::util::{merge_ranges, safe_range};

use super::{queuing::Queue, AudioEventChannel, AudioSource, DynamicBuffer, Sample};

pub type SourceId = u64;

/// This needs a better name. Maybe something like, scheduler?
/// It would be better to separate it out into its own thing.
pub struct SourceLoaderBuffer {
    events: AudioEventChannel,
    queue: Arc<Queue>,
    registry: SourceLoaderRegistry,
}

#[derive(Debug, Clone, Copy)]
pub enum LoaderEvent {
    /// Samples were read from the buffer with an offset.
    Read(usize),
    Advance,
}

impl SourceLoaderBuffer {
    /// How many samples to load after hitting the threshold.
    const PRELOAD_AMOUNT: usize = SAMPLES_PER_SEC * 60;

    /// The threshold at which loading more samples happens
    const PRELOAD_THRESHOLD: usize = SAMPLES_PER_SEC * 20;

    pub fn spawn(events: AudioEventChannel, queue: Arc<Queue>) -> Arc<Self> {
        let new_buffer = Self {
            queue,
            events,
            registry: SourceLoaderRegistry::new(),
        };

        let arced = Arc::new(new_buffer);

        thread::spawn({
            let buffer = Arc::clone(&arced);
            move || buffer.run()
        });

        arced
    }

    /// Reads samples into the buffer and returns the amount written and the new offset.
    /// This will block if there are pending ranges.
    pub fn read_samples(&self, offset: usize, buf: &mut [Sample]) -> (usize, usize) {
        let source = self.current_source();

        let requested_samples = buf.len();
        let samples_read = source.read_samples(offset, buf);

        if requested_samples == samples_read {
            return (samples_read, offset + samples_read);
        }

        // This is the end of the source.
        if source.is_complete() {
            self.advance();
            return (samples_read, 0);
        }

        // Otherwise, keep trying
        self.read_samples(offset + samples_read, &mut buf[samples_read..])
    }

    fn run(&self) {
        loop {
            let event = self.events.wait();

            match event {
                AudioEvent::Queue(e) => self.handle_queue_event(e),
                AudioEvent::Loader(e) => self.handle_loader_event(e),
            }
        }
    }

    fn handle_queue_event(&self, event: QueueEvent) {
        // To be implemented...
    }

    fn handle_loader_event(&self, event: LoaderEvent) {
        match event {
            LoaderEvent::Read(offset) => self.load_if_necessary(offset),
            LoaderEvent::Advance => todo!(),
        }
    }

    fn advance(&self) {
        self.queue.next();
    }

    fn load_more(&self, initial_offset: usize) {
        let mut offset = initial_offset;
        let mut remaining = Self::PRELOAD_AMOUNT;

        for source in self.sources() {
            let load_result = source.load_samples(offset..remaining);

            // In the next source we start loading from 0.
            offset = 0;

            match load_result {
                Ok(samples_read) => remaining.checked_sub(samples_read).unwrap_or_default(),
                Err(_err) => {
                    todo!("Handle source loading error")
                }
            };

            if remaining == 0 {
                break;
            }
        }
    }

    fn load_if_necessary(&self, offset: usize) {
        let amount_loaded = self.continuous_amount_loaded_ahead(offset);

        if amount_loaded < Self::PRELOAD_THRESHOLD {
            self.load_more(offset);
        }
    }

    /// Returns how many continuous loaded samples exist after the offset
    fn continuous_amount_loaded_ahead(&self, offset: usize) -> usize {
        let sources = self.sources();

        if sources.is_empty() {
            return 0;
        }

        let mut current_source = sources.first().unwrap();
        let mut current_amount = current_source.remaining_at_offset(offset);

        for source in sources.iter().skip(1) {
            if !current_source.is_continuous_with(source) {
                break;
            }

            current_amount += source.samples_loaded_from_start();
            current_source = source;
        }

        current_amount
    }

    fn should_skip(&self, offset: usize) -> bool {
        todo!("should_skip")
    }

    fn sources(&self) -> Vec<Arc<SourceLoader>> {
        self.queue
            .peek_ahead(10)
            .into_iter()
            .map(|t| self.registry.get_by_source(t.source()))
            .collect()
    }

    fn current_source(&self) -> Arc<SourceLoader> {
        let track = self.queue.current_track();
        self.registry.get_by_source(track.source())
    }
}

/// Keeps track of SourceLoaders, de-allocating them automatically
/// to save memory when necessary.
#[derive(Default)]
pub struct SourceLoaderRegistry {
    loaders: Mutex<Vec<Arc<SourceLoader>>>,
}

impl SourceLoaderRegistry {
    fn new() -> Self {
        Self {
            loaders: Default::default(),
        }
    }

    fn get_loader_by_id(&self, id: SourceId) -> Option<Arc<SourceLoader>> {
        let loaders = self.loaders.lock().unwrap();

        loaders.iter().find(|s| s.id == id).cloned()
    }

    fn add_loader<T>(&self, source: T) -> Arc<SourceLoader>
    where
        T: AudioSource,
    {
        let mut loaders = self.loaders.lock().unwrap();
        let loader = Arc::new(SourceLoader::new(source));

        loaders.push(loader.clone());
        loader
    }

    pub fn get_by_source<T>(&self, source: &T) -> Arc<SourceLoader>
    where
        T: AudioSource + Clone,
    {
        let id = source.id();

        if let Some(existing) = self.get_loader_by_id(id) {
            return existing;
        }

        self.add_loader(source.clone())
    }
}

/// Loads an AudioSource and keeps track of the ranges
/// that have been loaded.
pub struct SourceLoader {
    id: SourceId,
    source: Mutex<Box<dyn AudioSource>>,

    samples: Mutex<Vec<Sample>>,
    sample_ranges: Mutex<Vec<Range<usize>>>,

    err: Mutex<Option<SourceError>>,
}

impl SourceLoader {
    fn new<T: AudioSource>(source: T) -> Self {
        let length = source.length();
        let samples = vec![0.; length];

        Self {
            id: source.id(),
            source: Mutex::new(Box::new(source)),
            samples: samples.into(),
            sample_ranges: Default::default(),
            err: Mutex::new(None),
        }
    }

    fn load_samples(&self, range: Range<usize>) -> Result<usize, SourceError> {
        let mut buf = vec![0.; range.len()];
        let mut source = self.source.lock().unwrap();

        let samples_read = source.read_samples(range.start, &mut buf)?;

        // We lock samples after reading so that it isn't blocked from playing.
        let mut samples = self.samples.lock().unwrap();
        samples[..range.len()].copy_from_slice(&buf);

        Ok(samples_read)
    }

    fn read_samples(&self, offset: usize, buf: &mut [Sample]) -> usize {
        let remaining = self.remaining_at_offset(offset);
        let requested = buf.len();

        if remaining > 0 {
            let mut samples = self.samples.lock().unwrap();

            let start = offset;
            let end = start + remaining.min(requested);

            buf[..remaining].copy_from_slice(&samples[start..end]);

            (start..end).len()
        } else {
            0
        }
    }

    fn read__samples(&self, offset: usize, buf: &mut [Sample]) -> Result<usize, SourceError> {
        let mut amount_read = 0;

        amount_read += self.get_cached_samples(offset, buf);

        let new_offset = offset + amount_read;
        let new_buf = &mut buf[amount_read..];

        let mut source = self.source.lock().unwrap();
        amount_read += source.read_samples(new_offset, new_buf)?;

        Ok(amount_read)
    }

    fn get_cached_samples(&self, offset: usize, buf: &mut [Sample]) -> usize {
        let samples = self.samples.lock().unwrap();

        let range = {
            let ranges = self.sample_ranges.lock().unwrap();
            let range = ranges.iter().find(|r| r.contains(&offset));

            if let Some(range) = range {
                offset..range.len().min(buf.len())
            } else {
                // There are no cached samples at offset range
                return 0;
            }
        };

        let samples_written = range.len();

        buf.copy_from_slice(&samples[range]);
        samples_written
    }

    /// Submits samples to the cache, returning a copy of them.
    fn submit_samples(&self, offset: usize, new_samples: Vec<Sample>) -> Vec<Sample> {
        let mut samples = self.samples.lock().unwrap();

        let range = offset..new_samples.len();
        let range = safe_range(samples.len(), range);
        let len = range.len();

        self.submit_range(range.clone());

        // Safely copy the new samples to the cache
        samples[range].copy_from_slice(&new_samples[..len]);

        new_samples
    }

    /// Submits the range of loaded samples and merging with existing
    /// ranges if it is possible.
    fn submit_range(&self, new_range: Range<usize>) {
        let mut ranges = self.sample_ranges.lock().unwrap();

        *ranges = merge_ranges({
            ranges.push(new_range);
            ranges.to_vec()
        });
    }

    fn remaining_at_offset(&self, offset: usize) -> usize {
        let ranges = self.sample_ranges.lock().unwrap();

        ranges
            .iter()
            .find_map(|r| {
                r.contains(&offset).then(|| {
                    let used = offset.checked_sub(r.start).unwrap_or_default();
                    r.len() - used
                })
            })
            .unwrap_or_default()
    }

    fn is_complete(&self) -> bool {
        let ranges = self.sample_ranges.lock().unwrap();
        let samples = self.samples.lock().unwrap();

        ranges
            .get(0)
            .and_then(|r| Some(r.len() == samples.len()))
            .unwrap_or_default()
    }

    /// Returns true if this SourceLoader's range connects to `other`
    /// without interruptions.
    fn is_continuous_with(&self, other: &Self) -> bool {
        other
            .sample_start()
            .zip(self.sample_end())
            .map(|(start, end)| start == 0 && end == other.len())
            .unwrap_or_default()
    }

    fn samples_loaded_from_start(&self) -> usize {
        self.sample_ranges
            .lock()
            .unwrap()
            .first()
            .map(|r| r.len())
            .unwrap_or_default()
    }

    /// Returns the start of the first loaded range
    fn sample_start(&self) -> Option<usize> {
        let ranges = self.sample_ranges.lock().unwrap();
        ranges.first().map(|r| r.start)
    }

    /// Returns the end of the last loaded range
    fn sample_end(&self) -> Option<usize> {
        let ranges = self.sample_ranges.lock().unwrap();
        ranges.last().map(|r| r.end)
    }

    /// Returns the total length
    fn len(&self) -> usize {
        self.samples.lock().unwrap().len()
    }

    fn set_error(&self, new_err: Option<SourceError>) {
        let mut err = self.err.lock().unwrap();
        *err = new_err;
    }

    fn has_fatal_error(&self) -> bool {
        let err = self.err.lock().unwrap();
        err.as_ref().map(|e| e.is_fatal()).unwrap_or(false)
    }
}
