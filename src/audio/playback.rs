use std::{fs::File, path::Path, sync::Arc};

use super::{
    decoding::decode_to_raw, queuing::Queue, AudioEventChannel, SourceLoaderBuffer, CHANNEL_COUNT,
    SAMPLE_RATE,
};

/// Plays audio sources with no gaps
pub struct Player {
    events: AudioEventChannel,

    buffer: Arc<SourceLoaderBuffer>,
    queue: Arc<Queue>,

    sample_offset: usize,
}

impl Player {
    pub fn new(events: AudioEventChannel) -> Self {
        let queue = Arc::new(Queue::new(events.clone()));
        let buffer = SourceLoaderBuffer::spawn(events.clone(), queue.clone());

        Self {
            queue,
            buffer,
            events,
            sample_offset: Default::default(),
        }
    }

    pub fn add(&mut self, track: Track) {
        self.buffer.add_source(track.source.try_clone().unwrap());
    }

    /// Reads the next chunk of audio,
    /// returning the amount of samples written.
    pub fn read(&mut self, outgoing: &mut [f32]) -> usize {
        let (samples_read, new_offset) = self.buffer.read_samples(self.sample_offset, outgoing);

        self.sample_offset = new_offset;
        samples_read
    }

    pub fn current_time(&self) -> f32 {
        (self.sample_offset as f32) / ((SAMPLE_RATE * CHANNEL_COUNT) as f32)
    }

    pub fn current_track(&mut self) -> Option<Track> {
        todo!()
    }

    pub fn next_track(&mut self) -> Option<Track> {
        todo!()
    }
}

#[derive(Clone)]
pub struct Track {
    source: Arc<File>,
}

impl Track {
    pub fn from_file(path: &Path) -> Self {
        let file = File::open(&path).unwrap();
        let new_file = decode_to_raw(file, path.file_name().unwrap().to_str().unwrap());

        Self {
            source: new_file.into(),
        }
    }
}
