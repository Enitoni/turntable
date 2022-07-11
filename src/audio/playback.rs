use std::{
    fs::File,
    ops::Range,
    path::Path,
    sync::{Arc, Mutex},
};

use super::{decoding::decode_to_raw, AudioSource, AudioSourceLoader};

/// Plays audio sources with no gaps
// TODO: Make an abstraction for this sample concatenation
pub struct Player {
    loader: AudioSourceLoader,
    tracks: Vec<Track>,

    /// The relative offset of samples we are at now.
    sample_offset: usize,
}

impl Player {
    const CHUNK_SIZE: usize = 4096;

    /// At how many remaining samples should more be requested
    const PRELOAD_THRESHOLD: usize = 4096 * 4;

    pub fn new() -> Self {
        let loader = AudioSourceLoader::new();

        // Start loading
        loader.run();

        Self {
            loader,
            tracks: Default::default(),
            sample_offset: Default::default(),
        }
    }

    pub fn add(&mut self, track: Track) {
        self.tracks.push(track.clone());
        self.loader.request(track.source, Self::CHUNK_SIZE);
    }

    /// Reads the next chunk of audio
    pub fn next_chunk(&mut self, outgoing: &mut [f32]) {
        let mut consumed = 0;
        let mut current_track = self.current_track();

        while consumed < outgoing.len() {
            if current_track.is_none() {
                break;
            }

            let remaining = outgoing.len() - consumed;

            let start = self.sample_offset;
            let end = start + remaining;

            let is_finished = current_track.as_ref().map(|t| t.is_finished());
            let should_skip = is_finished.is_none() || is_finished.unwrap();

            dbg!(self.sample_offset);
            dbg!("oops", is_finished);

            // Schedule next track
            if should_skip {
                current_track = self.next_track();
                continue;
            }

            dbg!("SAMPLING!");

            let samples = current_track
                .as_ref()
                .map(|t| t.samples(start..end))
                .unwrap();

            println!("GOT SAMPLES {}", samples.len());

            outgoing[consumed..samples.len()].copy_from_slice(&samples);
            consumed += samples.len();
        }

        self.sample_offset += consumed;

        // TODO: This is temporary loading until a better abstraction
        // for this whole thing is made.
        if let Some(track) = current_track {
            self.loader.request(track.source, Self::CHUNK_SIZE * 4);
        }
    }

    pub fn current_track(&mut self) -> Option<Track> {
        self.tracks.first().map(|t| t.clone())
    }

    pub fn next_track(&mut self) -> Option<Track> {
        dbg!("NEW TRACK");
        self.sample_offset = 0;
        self.tracks.pop().map(|t| t.clone())
    }
}

#[derive(Clone)]
pub struct Track {
    source: Arc<Mutex<AudioSource>>,
}

impl Track {
    pub fn samples(&self, range: Range<usize>) -> Vec<f32> {
        let mut outgoing = vec![0.; range.len()];
        let mut consumed = 0;

        // Keep checking if data is missing but source is not finished
        while consumed < outgoing.len() {
            let mut source = self.source.lock().unwrap();
            let start = consumed + range.start;

            let samples = source.samples(start..range.end);
            let end = outgoing.len().min(samples.len());

            outgoing[consumed..end].copy_from_slice(samples);
            consumed += samples.len();

            if source.is_finished() {
                break;
            }
        }

        outgoing
    }

    pub fn is_finished(&self) -> bool {
        let source = self.source.lock().unwrap();
        source.is_finished()
    }

    pub fn from_file(path: &Path) -> Self {
        let file = File::open(&path).unwrap();
        let new_file = decode_to_raw(file, path.file_name().unwrap().to_str().unwrap());

        let source = AudioSource::new(new_file);
        Self {
            source: Arc::new(source.into()),
        }
    }
}
