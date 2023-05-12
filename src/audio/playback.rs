use std::{
    sync::{Arc, Weak},
    thread,
    time::{Duration, Instant},
};

use crate::{
    ingest::{Sink, SinkId},
    store::{FromId, Id, Store},
    EventEmitter,
};
use dashmap::DashMap;
use log::warn;

use super::{
    new::{Stream, StreamConsumer},
    AudioEvent, Timeline, PRELOAD_AMOUNT, SAMPLES_PER_SEC, STREAM_CHUNK_DURATION,
    STREAM_CHUNK_SIZE,
};

pub type PlayerId = Id<Player>;

/// Handles playback for a list of sinks.
#[derive(Debug)]
pub struct Player {
    id: PlayerId,
    timeline: Timeline,
    stream: Arc<Stream>,
}

impl Player {
    /// Set the sinks to play.
    ///
    /// **Note that the first sink is the one currently being played.**
    pub fn set_sinks(&self, sinks: Vec<Sink>) {
        self.timeline.set_sinks(sinks);
    }

    /// Get a new consumer of the underlying stream.
    pub fn consumer(&self) -> StreamConsumer {
        self.stream.consumer()
    }

    /// Return the sink to preload, if any
    pub fn preload(&self) -> Option<SinkId> {
        self.timeline.preload()
    }

    /// Advance the playback by reading from sinks and pushing samples into a ringbuffer.
    /// Returns information about the advancement.
    pub fn process(&self) -> ProcessMetadata {
        let mut samples = vec![0.; STREAM_CHUNK_SIZE];

        let current_offset = self.timeline.offset.load();
        let advancements = self.timeline.advance(samples.len());

        let mut amount_read = 0;
        let consumed_sinks = advancements.len().saturating_sub(1);

        for (i, advancement) in advancements.into_iter().enumerate() {
            amount_read += advancement
                .sink
                .read(advancement.start_offset, &mut samples[amount_read..]);

            if i < consumed_sinks && consumed_sinks >= 1 {
                advancement.sink.consume();
            }
        }

        self.stream.write(&samples);

        let new_sink_offset = self.timeline.offset.load();
        let total_offset = self.timeline.total_offset.load();

        let difference = new_sink_offset.saturating_sub(current_offset);

        ProcessMetadata {
            new_sink_offset,
            consumed_sinks,
            total_offset,
            difference,
        }
    }
}

impl Default for Player {
    fn default() -> Self {
        Self {
            id: PlayerId::new(),
            timeline: Timeline::default(),
            stream: Stream::new(),
        }
    }
}

impl FromId<PlayerId> for Arc<Player> {
    fn from_id(store: &Store, id: &PlayerId) -> Option<Self>
    where
        Self: Sized,
    {
        store.playback.players.get(id).map(|p| p.clone())
    }
}

/// Describes the result of processing a chunk.
#[derive(Debug, Clone)]
pub struct ProcessMetadata {
    /// Offset of the sink currently playing.
    pub new_sink_offset: usize,

    /// Total offset of the entire timeline
    pub total_offset: usize,

    /// Difference in this offset to the last one.
    /// This will be 0 if the player has reached the end, is waiting for sinks to be loaded, or is paused.
    pub difference: usize,

    /// The amount of sinks that were fully played.
    ///
    /// This is 1 or more when the player has finished playing a track, otherwise it is usually 0.
    ///
    /// **Note that finished can also mean the sink was skipped due to an error.**
    pub consumed_sinks: usize,
}

#[derive(Debug)]
pub struct Playback {
    store: Weak<Store>,
    emitter: EventEmitter,
    players: DashMap<PlayerId, Arc<Player>>,
}

impl Playback {
    /// How many players can exist at once
    const MAXIMUM_PLAYERS: usize = 16;

    pub fn new(store: Weak<Store>, emitter: EventEmitter) -> Self {
        Self {
            store,
            emitter,
            players: Default::default(),
        }
    }

    pub fn create_player(&self) -> Result<PlayerId, String> {
        let amount_of_players = self.players.len();

        if amount_of_players == Self::MAXIMUM_PLAYERS {
            return Err("Exceeded maximum players".to_string());
        }

        let new_player = Player::default();
        let id = new_player.id;

        self.players.insert(id, new_player.into());

        Ok(id)
    }

    fn store(&self) -> Arc<Store> {
        self.store.upgrade().expect("upgrade store in playback")
    }

    fn players(&self) -> Vec<Arc<Player>> {
        self.players.iter().map(|p| p.clone()).collect()
    }

    fn preload(&self) {
        let sinks: Vec<_> = self
            .players()
            .into_iter()
            .filter_map(|p| p.preload())
            .collect();

        for sink in sinks {
            self.store().ingestion.request(sink, PRELOAD_AMOUNT)
        }
    }

    fn process(&self) {
        let emitter = self.emitter.clone();

        for player in self.players() {
            let processed = player.process();

            if processed.difference > 0 {
                emitter.dispatch(AudioEvent::Time {
                    player: player.id,
                    total_offset: processed.total_offset,
                    offset: processed.new_sink_offset,
                })
            }

            for _ in 0..processed.consumed_sinks {
                emitter.dispatch(AudioEvent::Next { player: player.id })
            }
        }
    }
}

fn spawn_preload_thread(playback: Arc<Playback>) {
    let run = move || loop {
        playback.preload();
        thread::sleep(Duration::from_millis(100));
    };

    thread::Builder::new()
        .name("audio-preload".to_string())
        .spawn(run)
        .unwrap();
}

fn spawn_processing_thread(playback: Arc<Playback>) {
    let run = move || loop {
        let now = Instant::now();
        playback.process();
        wait_for_next(now);
    };

    thread::Builder::new()
        .name("audio-processing".to_string())
        .spawn(run)
        .unwrap();
}

fn wait_for_next(now: Instant) {
    let elapsed = now.elapsed();
    let elapsed_micros = elapsed.as_micros();
    let elapsed_millis = elapsed_micros / 1000;

    let duration_micros = STREAM_CHUNK_DURATION.as_micros();

    if elapsed_millis > SAMPLES_PER_SEC as u128 / 10000 {
        warn!(
            "Stream took too long ({}ms) to process samples!",
            elapsed_millis
        )
    }

    let corrected = duration_micros
        .checked_sub(elapsed_micros)
        .unwrap_or_default();

    spin_sleep::sleep(Duration::from_micros(corrected as u64));
}

pub fn run_playback(playback: Arc<Playback>) {
    spawn_preload_thread(playback.clone());
    spawn_processing_thread(playback);
}
