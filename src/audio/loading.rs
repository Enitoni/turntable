use std::{
    fmt::Display,
    ops::Range,
    os::windows::prelude::FileExt,
    sync::{
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread,
};

use crate::util::{merge_ranges, safe_range};

use super::{DynamicBuffer, Sample, CHANNEL_COUNT, SAMPLE_RATE};

pub type SourceId = u32;

pub struct SourceLoaderBuffer {
    buffer: DynamicBuffer<SourceId>,
    sources: Mutex<Vec<SourceLoader>>,

    sender: SyncSender<LoaderCommand>,
}

#[derive(Debug)]
pub enum ReadResult {
    /// Samples were read successfully, and there may be more content.
    Read {
        samples_read: usize,
        new_offset: usize,
    },
    /// No data was read, because the chunk must be skipped.
    /// This is usually due to an error.
    Skip { new_offset: usize },
}

pub enum LoaderCommand {
    Load(SourceId, Range<usize>),
}

impl SourceLoaderBuffer {
    const COMMAND_BUFFER_SIZE: usize = 256;

    const PRELOAD_PADDING: usize = SAMPLE_RATE * CHANNEL_COUNT * 60;
    const PRELOAD_THRESHOLD: usize = SAMPLE_RATE * CHANNEL_COUNT * 10;

    pub fn spawn() -> Arc<Self> {
        let (sender, receiver) = sync_channel(Self::COMMAND_BUFFER_SIZE);

        let new_buffer = Self {
            buffer: DynamicBuffer::new(),
            sources: Default::default(),
            sender,
        };

        let arced = Arc::new(new_buffer);

        thread::spawn({
            let buffer = Arc::clone(&arced);
            move || buffer.process_commands(receiver)
        });

        arced
    }

    pub fn add_source<T: 'static + SampleSource + Send + Sync>(&self, source: T) {
        let mut sources = self.sources.lock().unwrap();
        let new_id = (sources.len() + 1) as SourceId;

        let length = source.length();
        let loader = SourceLoader::new(new_id, source);

        // Temporary
        dbg!("Remember to remove me!");
        let _ = self.sender.send(LoaderCommand::Load(new_id, 0..length));

        self.buffer.allocate(new_id, length);
        sources.push(loader);
    }

    /// Reads samples into the buffer and returns the amount written and the new offset.
    /// This will block if there are pending ranges.
    pub fn read_samples(&self, offset: usize, buf: &mut [Sample]) -> (usize, usize) {
        let result = self.buffer.read_samples(offset, buf);

        match result {
            super::ReadBufferSamplesResult::All { samples_read } => {
                (samples_read, offset + samples_read)
            }
            super::ReadBufferSamplesResult::Partial {
                samples_read,
                skip_offset,
            } => {
                if self.should_skip(offset) {
                    (samples_read, skip_offset)
                } else {
                    self.read_samples(offset + samples_read, &mut buf[samples_read..])
                }
            }
            super::ReadBufferSamplesResult::End { samples_read } => {
                self.read_samples(0, &mut buf[samples_read..])
            }
        }
    }

    pub fn process_commands(&self, receiver: Receiver<LoaderCommand>) {
        while let Ok(command) = receiver.recv() {
            match command {
                LoaderCommand::Load(source_id, range) => self.attempt_load_source(source_id, range),
            }
        }
    }

    fn should_skip(&self, offset: usize) -> bool {
        let id = self.buffer.id_at_offset(offset);

        if let Some(id) = id {
            let sources = self.sources.lock().unwrap();

            sources
                .iter()
                .find_map(|s| (s.id == id).then(|| s.has_fatal_error()))
                .unwrap_or(false)
        } else {
            false
        }
    }

    fn attempt_load_source(&self, source_id: SourceId, range: Range<usize>) {
        let sources = self.sources.lock().unwrap();
        let source = sources
            .iter()
            .find(|s| s.id == source_id)
            .expect("Source exists in the SourceLoaderBuffer");

        let samples_to_load = range.len();
        let offset = range.start;

        let mut buf = vec![0.; samples_to_load];
        let mut samples_read = 0;

        let result = source.read_samples(offset, &mut buf);

        match result {
            Ok(loaded) => {
                source.set_error(None);

                samples_read += loaded;
                self.buffer.write_samples(source_id, offset, &buf);
            }
            Err(err) => {
                source.set_error(Some(err.clone()));

                if err.is_fatal() {
                    return;
                }
            }
        }

        // Try loading again later
        if samples_read < samples_to_load {
            let start = samples_read;
            let end = start + (samples_to_load - samples_read);

            let _ = self.sender.send(LoaderCommand::Load(source_id, start..end));
        }
    }
}

/// Loads a SampleSource and keeps track of the ranges
/// that have been loaded.
pub struct SourceLoader {
    id: SourceId,
    source: Mutex<Box<dyn SampleSource + Send + Sync>>,

    samples: Mutex<Vec<Sample>>,
    sample_ranges: Mutex<Vec<Range<usize>>>,

    err: Mutex<Option<SampleSourceError>>,
}

impl SourceLoader {
    fn new<T: 'static + SampleSource + Send + Sync>(id: SourceId, source: T) -> Self {
        let length = source.length();
        let samples = vec![0.; length];

        Self {
            id,
            source: Mutex::new(Box::new(source)),
            samples: samples.into(),
            sample_ranges: Default::default(),
            err: Mutex::new(None),
        }
    }

    fn read_samples(&self, offset: usize, buf: &mut [Sample]) -> Result<usize, SampleSourceError> {
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

    fn set_error(&self, new_err: Option<SampleSourceError>) {
        let mut err = self.err.lock().unwrap();
        *err = new_err;
    }

    fn has_fatal_error(&self) -> bool {
        let err = self.err.lock().unwrap();
        err.as_ref().map(|e| e.is_fatal())
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone)]
pub enum SampleSourceError {
    /// Loading samples failed for whatever reason.
    /// This could be network related, for example.
    LoadFailed { reason: String },
    /// The SampleSource cannot be used anymore,
    /// and should be skipped.
    Fatal { reason: String },
}

impl SampleSourceError {
    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::Fatal { .. })
    }
}

impl Display for SampleSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SampleSourceError::LoadFailed { reason } => {
                write!(f, "Source failed to load samples: \"{}\"", reason)?;
            }
            SampleSourceError::Fatal { reason } => write!(
                f,
                "Source is skipped because of fatal error: \"{}\"",
                reason
            )?,
        };

        Ok(())
    }
}

/// A source of audio samples
pub trait SampleSource {
    /// How many samples does this source return
    fn length(&self) -> usize;

    fn read_samples(&mut self, offset: usize, buf: &mut [f32]) -> Result<usize, SampleSourceError>;
}

impl SampleSource for std::fs::File {
    fn length(&self) -> usize {
        let meta = self.metadata().expect("Read metadata");

        (meta.len() as usize) / 4
    }

    fn read_samples(&mut self, offset: usize, buf: &mut [f32]) -> Result<usize, SampleSourceError> {
        let mut intermediate = vec![0; buf.len() * 4];

        self.seek_read(&mut intermediate, offset as u64)
            .map_err(|e| SampleSourceError::LoadFailed {
                reason: e.to_string(),
            })?;

        let samples: Vec<_> = intermediate
            .chunks_exact(4)
            .map(|byte| f32::from_le_bytes([byte[0], byte[1], byte[2], byte[3]]))
            .collect();

        buf[..samples.len()].copy_from_slice(&samples);
        Ok(samples.len())
    }
}
