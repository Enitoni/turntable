use log::{info, warn};
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

mod buffering;
mod decoding;
mod encoding;
mod playback;
mod processing;
mod queuing;
mod source;
mod track;
pub mod util;

pub use buffering::*;
pub use decoding::raw_samples_from_bytes;
pub use encoding::*;
pub use ingest::Input;
pub use playback::*;
pub use queuing::Queue;
pub use track::Track;
pub use util::pipeline;

#[derive(Clone)]
pub struct AudioSystem {
    ingestion: Arc<Ingestion>,
    queue: Arc<Queue>,
    registry: Arc<buffering::BufferRegistry>,
    scheduler: Arc<playback::Scheduler>,
}

impl AudioSystem {
    pub fn new() -> Arc<Self> {
        let queue = Queue::new();
        let ingestion = Ingestion::new();

        Self {
            registry: buffering::BufferRegistry::new().into(),
            scheduler: playback::Scheduler::new().into(),
            ingestion: ingestion.into(),
            queue: queue.into(),
        }
        .into()
    }

    pub fn stream(&self) -> AudioBufferConsumer {
        self.registry.get_consumer()
    }

    pub fn add(&self, input: Input) {
        let sink = self.ingestion.add(input.loader());

        // This is temporary for now
        let track = Track::new(sink);
        self.queue.add_track(track, queuing::QueuePosition::Add);

        let new_sinks: Vec<_> = self
            .queue
            .peek_ahead(5)
            .into_iter()
            .map(|t| t.sink)
            .collect();

        self.scheduler.set_sinks(new_sinks);
        self.notify_queue_update();
    }

    pub fn next(&self) {
        self.queue.next();
        self.notify_queue_update();
    }

    fn notify_queue_update(&self) {
        let new_sinks: Vec<_> = self
            .queue
            .peek_ahead(5)
            .into_iter()
            .map(|t| t.sink)
            .collect();

        self.scheduler.set_sinks(new_sinks);
    }
}

pub fn spawn_audio_thread(system: Arc<AudioSystem>) {
    ingest::spawn_loading_thread(system.ingestion.clone());
    ingest::spawn_processing_thread(system.ingestion.clone());
    ingest::spawn_load_write_thread(system.ingestion.clone());

    spawn_scheduler_load_check_thread(system.clone());
    spawn_playback_thread(system);
}

pub fn spawn_scheduler_load_check_thread(system: Arc<AudioSystem>) {
    let scheduler = system.scheduler.clone();
    let ingestion = system.ingestion.clone();

    let run = move || {
        info!("Now listening for load requests",);

        loop {
            let requests = scheduler.preload().first().cloned();

            if let Some((id, amount)) = requests {
                ingestion.request(id, amount);
                scheduler.notify_load();
            }

            thread::sleep(Duration::from_millis(500));
        }
    };

    thread::Builder::new()
        .name("audio_loading".to_string())
        .spawn(run)
        .unwrap();
}

pub fn spawn_playback_thread(system: Arc<AudioSystem>) {
    let ingestion = system.ingestion.clone();
    let scheduler = system.scheduler.clone();
    let read_samples_system = system.clone();

    let read_samples = move |buf: &mut [Sample]| {
        let advancements = scheduler.advance(buf.len());
        let mut amount_read = 0;

        for (id, range) in advancements.iter() {
            let sink = ingestion.get(*id);
            amount_read += sink.read(range.start, &mut buf[amount_read..]);
        }

        for (_, _) in advancements.iter().skip(1) {
            read_samples_system.next();
        }
    };

    let system = system.clone();

    let tick = move || {
        let mut samples = vec![0.; STREAM_CHUNK_SIZE];
        read_samples(&mut samples);

        let samples_as_bytes: Vec<_> = samples
            .into_iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();

        system.registry.write_byte_samples(&samples_as_bytes);
    };

    thread::Builder::new()
        .name("audio_stream".to_string())
        .spawn(move || {
            info!(
                "Now processing {} samples per {}ms ({} sample/s) at {:.1} kHz",
                STREAM_CHUNK_SIZE,
                STREAM_CHUNK_DURATION.as_millis(),
                SAMPLES_PER_SEC,
                SAMPLE_RATE as f32 / 1000.
            );

            loop {
                let now = Instant::now();
                tick();

                wait_for_next(now);
            }
        })
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

mod config {
    use std::time::Duration;

    pub type Sample = f32;
    // pub const PCM_MIME: &str = "audio/pcm;rate=44100;encoding=float;bits=32";

    pub const SAMPLE_RATE: usize = 44100;
    pub const CHANNEL_COUNT: usize = 2;

    pub const SAMPLE_IN_BYTES: usize = 4;
    pub const SAMPLES_PER_SEC: usize = SAMPLE_RATE * CHANNEL_COUNT;

    pub const STREAM_CHUNK_DURATION: Duration = Duration::from_millis(100);
    pub const STREAM_CHUNK_SIZE: usize =
        (((SAMPLES_PER_SEC as u128) * STREAM_CHUNK_DURATION.as_millis()) / 1000) as usize;
}

pub use config::*;

use crate::ingest::{self, Ingestion};
