use std::sync::Mutex;

use super::{AudioEvent, AudioEventChannel, Track};

pub struct Queue {
    events: AudioEventChannel,

    tracks: Mutex<Vec<Track>>,
    index: Mutex<usize>,
}

#[derive(Debug, Clone, Copy)]
pub enum QueueEvent {
    /// A request to advance the queue by x amount was made.
    /// This can either be automatic or caused by a user.
    Advance { amount: isize, new_index: usize },
    /// The queue was updated from adding, or removing a track.
    Update,
}

pub enum QueuePosition {
    Next,
}

impl Queue {
    pub fn new(events: AudioEventChannel) -> Self {
        Self {
            events,
            tracks: Default::default(),
            index: Default::default(),
        }
    }

    pub fn add_track(&self, track: Track, position: QueuePosition) {
        let current_index = self.current_index();
        let mut tracks = self.tracks.lock().unwrap();

        match position {
            QueuePosition::Next => {
                let at = self.index_at(current_index + 1);
                tracks.insert(at, track);
            }
        };

        self.events.emit(QueueEvent::Update);
    }

    /// Advance the queue, returning the next track
    pub fn next(&self) -> Track {
        self.advance_index(1);
        self.current_track()
    }

    pub fn peek_ahead(&self, amount: usize) -> Vec<Track> {
        let current_index = self.current_index();
        let tracks = self.tracks.lock().unwrap();

        (0..amount)
            .into_iter()
            .map(|i| self.index_at(current_index + i))
            .map(|i| tracks.get(i).cloned().expect("Track is in queue"))
            .collect()
    }

    pub fn current_track(&self) -> Track {
        let tracks = self.tracks.lock().unwrap();

        tracks
            .get(self.current_index())
            .cloned()
            .expect("Current track exists in queue")
    }

    fn advance_index(&self, advance: isize) {
        let mut current_index = self.index.lock().unwrap();

        let advanced_index = *current_index as isize + advance;
        let new_index = self.index_at(advanced_index as usize);

        *current_index = new_index;

        self.events.emit(QueueEvent::Advance {
            amount: advance,
            new_index,
        });
    }

    fn set_index(&self, new_index: usize) {
        let mut current_index = self.index.lock().unwrap();
        let new_index = self.index_at(new_index);

        *current_index = new_index;
    }

    fn current_index(&self) -> usize {
        let current_index = self.index.lock().unwrap();
        *current_index
    }

    /// Returns the index in a cyclic manner
    fn index_at(&self, index: usize) -> usize {
        let tracks = self.tracks.lock().unwrap();
        let normalized = index % tracks.len();

        normalized
    }
}

impl From<QueueEvent> for AudioEvent {
    fn from(e: QueueEvent) -> Self {
        AudioEvent::Queue(e)
    }
}
