use log::info;
use std::sync::Arc;

mod buffering;
mod decoding;
mod encoding;
mod events;
mod input;
mod loading;
mod playback;
mod processing;
mod queuing;
mod source;
mod track;
mod util;

pub use buffering::*;
pub use decoding::raw_samples_from_bytes;
pub use encoding::*;
pub use events::*;
pub use input::Input;
pub use loading::*;
pub use playback::*;
pub use queuing::Queue;
pub use track::Track;
pub use util::pipeline;

#[derive(Clone)]
pub struct AudioSystem {
    events: AudioEventChannel,
    queue: Arc<Queue>,
    registry: Arc<buffering::BufferRegistry>,
    scheduler: Arc<playback::Scheduler>,
    pool: Arc<loading::Pool>,
}

impl AudioSystem {
    fn new() -> Self {
        let events = AudioEventChannel::new();

        let queue: Arc<_> = Queue::new(events.clone()).into();

        Self {
            events,
            registry: buffering::BufferRegistry::new().into(),
            scheduler: playback::Scheduler::new().into(),
            pool: loading::Pool::new().into(),
            queue,
        }
    }

    pub fn stream(&self) -> AudioBufferConsumer {
        self.registry.get_consumer()
    }

    pub fn start(&self) {
        info!("Starting audio system");

        playback_thread::start(self);
        loading_thread::start(self);
    }
}

impl Default for AudioSystem {
    fn default() -> Self {
        Self::new()
    }
}

mod playback_thread {
    use std::thread;
    use std::time::{Duration, Instant};

    use log::{info, warn};

    use super::config::*;
    use super::AudioSystem;

    /// Starts the thread which will process samples in real-time
    pub fn start(system: &AudioSystem) {
        let scheduler = system.scheduler.clone();
        let registry = system.registry.clone();
        let pool = system.pool.clone();

        let read_samples = move |buf: &mut [Sample]| {
            let advancements = scheduler.advance(buf.len());
            let amount_to_advance = advancements.len().checked_sub(1).unwrap_or_default();

            for (id, range) in advancements {
                pool.read(id, range.start, &mut buf[range]);
            }

            for _ in 0..amount_to_advance {
                todo!()
            }
        };

        let tick = move || {
            let mut samples = vec![0.; STREAM_CHUNK_SIZE];
            read_samples(&mut samples);

            let samples_as_bytes: Vec<_> = samples
                .into_iter()
                .flat_map(|sample| sample.to_le_bytes())
                .collect();

            registry.write_byte_samples(&samples_as_bytes);
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
}

mod loading_thread {
    use std::{thread, time::Duration};

    use log::info;

    use super::AudioSystem;

    // Starts the thread that will load sources
    pub fn start(system: &AudioSystem) {
        let scheduler = system.scheduler.clone();
        let pool = system.pool.clone();

        thread::Builder::new()
            .name("audio_loading".to_string())
            .spawn(move || {
                info!("Now listening for loaders requested by scheduler",);

                loop {
                    let requests = scheduler.preload();

                    for (id, amount) in requests {
                        let new_amount = pool.load(id, amount);
                        scheduler.handle_load(id, new_amount);
                    }

                    thread::sleep(Duration::from_millis(500));
                }
            })
            .unwrap();
    }
}

mod config {
    use std::time::Duration;

    pub type Sample = f32;
    pub const PCM_MIME: &str = "audio/pcm;rate=44100;encoding=float;bits=32";

    pub const SAMPLE_RATE: usize = 44100;
    pub const CHANNEL_COUNT: usize = 2;

    pub const SAMPLE_IN_BYTES: usize = 4;
    pub const SAMPLES_PER_SEC: usize = SAMPLE_RATE * CHANNEL_COUNT;
    pub const BYTES_PER_SAMPLE: usize = 4 * SAMPLE_IN_BYTES;

    pub const STREAM_CHUNK_DURATION: Duration = Duration::from_millis(100);
    pub const STREAM_CHUNK_SIZE: usize =
        (((SAMPLES_PER_SEC as u128) * STREAM_CHUNK_DURATION.as_millis()) / 1000) as usize;
}

pub use config::*;
