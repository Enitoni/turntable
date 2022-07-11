use std::{
    ops::Range,
    sync::{Arc, Mutex},
};

use super::{AudioSource, AudioSourceLoader};

/// Plays audio sources with no gaps
// TODO: Make an abstraction for this sample concatenation
pub struct Player {
    loader: AudioSourceLoader,
    tracks: Vec<Track>,

    /// The relative offset of samples we are at now.
    sample_offset: usize,
}

impl Player {
    const CHUNK_SIZE: usize = 4096 * 2;

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
        self.tracks.push(track);
    }

    /// Reads the next chunk of audio
    pub fn next_chunk(&mut self, outgoing: &mut [f32]) {
        let mut consumed = 0;
        let mut current_track = self.current_track();

        while consumed < outgoing.len() {
            let remaining = outgoing.len() - consumed;

            let samples = current_track
                .as_ref()
                .map(|t| t.samples(self.sample_offset..remaining));

            // There are no more tracks.
            if samples.is_none() {
                break;
            }

            let samples = samples.unwrap();

            // Schedule next track
            if samples.is_empty() {
                current_track = self.next_track();
                continue;
            }

            outgoing[consumed..samples.len()].copy_from_slice(&samples);
            consumed += samples.len();
        }

        // TODO: This is temporary loading until a better abstraction
        // for this whole thing is made.
        if let Some(track) = current_track {
            self.loader.request(track.source, Self::CHUNK_SIZE);
        }
    }

    pub fn current_track(&mut self) -> Option<Track> {
        self.tracks.first().map(|t| t.clone())
    }

    pub fn next_track(&mut self) -> Option<Track> {
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
            let source = self.source.lock().unwrap();

            let start = consumed + range.start;
            let samples = source.samples(start..range.end);

            outgoing[consumed..samples.len()].copy_from_slice(samples);
            consumed += samples.len();

            if source.is_finished() {
                break;
            }
        }

        outgoing
    }
}
