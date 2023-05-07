use log::{info, warn};
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use surrealdb::sql::Thing;

mod buffering;
mod decoding;
mod encoding;
mod playback;
mod processing;
mod queuing;
mod router;
mod source;
mod track;
pub mod util;

pub use buffering::*;
pub use decoding::raw_samples_from_bytes;
pub use encoding::*;
pub use ingest::Input;
pub use playback::*;
pub use queuing::Queue;
pub use router::router;
pub use track::Track;
pub use util::pipeline;

#[derive(Clone)]
pub struct AudioSystem {
    events: Events,
    ingestion: Arc<Ingestion>,
    queue: Arc<Queue>,
    registry: Arc<buffering::BufferRegistry>,
    scheduler: Arc<playback::Scheduler>,
}

impl AudioSystem {
    pub fn new(events: Events) -> Arc<Self> {
        let queue = Queue::new();
        let ingestion = Ingestion::new();

        Self {
            events,
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

    pub fn add(&self, user_id: UserId, input: Input) {
        // This is temporary for now
        let sink = self.ingestion.add(input.loader().unwrap());
        let track = Track::new(input.duration(), input.to_string(), sink);
        self.queue
            .add_track(track.clone(), queuing::QueuePosition::Add);

        self.events.emit(
            Event::QueueAdd {
                user: user_id,
                track,
            },
            Recipients::All,
        );

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
        self.queue.current_track().sink.consume();
        self.queue.next();

        self.events.emit(
            Event::TrackUpdate {
                room: Thing::from(("undefined", "undefined")),
                track: self.queue.current_track(),
            },
            Recipients::All,
        );

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
    ingest::spawn_cleanup_thread(system.ingestion.clone());

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

        read_samples_system.events.emit(
            Event::PlayerTime {
                seconds: scheduler.current_seconds(),
                timestamp: "".to_string(),
            },
            Recipients::All,
        )
    };

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

use crate::{
    auth::UserId,
    events::{Event, Events},
    ingest::{self, Ingestion},
    server::ws::Recipients,
};

mod new {

    use std::sync::Weak;
    use std::{fmt::Debug, sync::Arc};

    use crossbeam::atomic::AtomicCell;
    use parking_lot::{Mutex, RwLock};
    use ringbuf::{Consumer, Producer, RingBuffer};

    use crate::ingest::{Sink, SinkId};
    use crate::util::ID_COUNTER;

    use super::{Sample, SAMPLES_PER_SEC};

    /// Handles playback for a list of sinks.
    #[derive(Debug)]
    pub struct Player {
        timeline: Timeline,
        stream: Arc<Stream>,
    }

    impl Player {
        /// Set the sinks to play.
        ///
        /// **Note that the first sink is the one currently being played.**
        pub fn set_sinks(&self, sinks: Vec<Sink>) {
            todo!()
        }

        /// Get a new consumer of the underlying stream.
        pub fn consumer(&self) -> StreamConsumer {
            self.stream.consumer()
        }

        /// Advance the playback by reading from sinks and pushing samples into a ringbuffer.
        /// Returns information about the advancement.
        pub fn process(&self) -> ProcessMetadata {
            todo!()
        }
    }

    impl Default for Player {
        fn default() -> Self {
            Self {
                timeline: Timeline::default(),
                stream: Stream::new(),
            }
        }
    }

    /// Describes the result of processing a chunk.
    #[derive(Debug, Clone)]
    pub struct ProcessMetadata {
        /// Offset of the sink currently playing.
        pub new_sink_offset: usize,

        /// Difference in this offset to the last one.
        /// This will be 0 if the player has reached the end, is waiting for sinks to be loaded, or is paused.
        pub difference: usize,

        /// List of sinks that need more samples
        ///
        /// When the player gets close to the end of available data
        /// this Vec will contain the id of one or more sinks that need to be loaded to.
        pub needs_loading: Vec<SinkId>,

        /// The amount of sinks that were fully played.
        ///
        /// This is 1 or more when the player has finished playing a track, otherwise it is usually 0.
        ///
        /// **Note that finished can also mean the sink was skipped due to an error.**
        pub consumed_sinks: usize,
    }

    /// A list of consecutive sinks that keeps track of offset and amount loaded.
    ///
    /// This is responsible for advancing the playback.
    #[derive(Debug, Default)]
    pub struct Timeline {
        /// A sequence of sinks. The first one is the currently playing one.
        sinks: Mutex<Vec<Sink>>,
        /// Offset of the current sink.
        offset: AtomicCell<usize>,
        /// The total amount of samples that have been advanced.
        total_offset: AtomicCell<usize>,
        /// Amount of contiguous available samples.
        total_available: AtomicCell<usize>,
    }

    impl Timeline {
        pub fn set_sinks(&self, sinks: Vec<Sink>) {
            *self.sinks.lock() = sinks
        }

        /// Advance the timeline and return a list of advancements
        /// describing sinks to read from or load to.
        pub fn advance(&self) -> Vec<Advancement> {
            todo!()
        }
    }

    #[derive(Debug)]
    pub enum Advancement {
        /// Read from a sink at a specific offset.
        Read {
            sink: Sink,
            start_offset: usize,
            end_offset: usize,
        },
        /// Request loading into a sink.
        Load { sink: SinkId, amount: usize },
    }

    type StreamConsumerId = u64;

    /// Represents a stream of audio that can be consumed from multiple places.
    pub struct Stream {
        me: Weak<Stream>,
        entries: Mutex<Vec<(StreamConsumerId, Producer<Sample>)>>,

        /// Preloaded samples a consumer will be filled with
        preloaded: RwLock<[Sample; SAMPLES_PER_SEC]>,
    }

    impl Stream {
        pub fn new() -> Arc<Self> {
            Arc::new_cyclic(|me| Stream {
                me: me.clone(),
                entries: Default::default(),
                preloaded: RwLock::new([0f32; SAMPLES_PER_SEC]),
            })
        }

        /// Create a new consumer preloaded with samples
        pub fn consumer(&self) -> StreamConsumer {
            let buffer = RingBuffer::new(SAMPLES_PER_SEC);

            let (mut producer, consumer) = buffer.split();

            let preloaded = self.preloaded.read();
            producer.push_slice(&*preloaded);

            let stream_consumer = StreamConsumer {
                id: ID_COUNTER.fetch_add(1),
                stream: self.me.clone(),
                underlying: consumer,
            };

            self.entries.lock().push((stream_consumer.id, producer));
            stream_consumer
        }

        /// Write samples to all consumers and the preload
        pub fn write(&self, buf: &[Sample]) {
            let mut entries = self.entries.lock();

            for (_, producer) in entries.iter_mut() {
                producer.push_slice(buf);
            }

            let mut preloaded = self.preloaded.write();
            let slice = &mut preloaded[..buf.len()];
            slice.copy_from_slice(buf);
        }

        fn remove(&self, id: StreamConsumerId) {
            self.entries.lock().retain(|(i, _)| i != &id);
        }
    }

    impl Debug for Stream {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "(Stream)")
        }
    }

    /// A consumer of a [Stream]
    pub struct StreamConsumer {
        id: StreamConsumerId,
        stream: Weak<Stream>,
        underlying: Consumer<Sample>,
    }

    impl StreamConsumer {
        /// Read from the consumer, returning how many samples were read
        pub fn read(&mut self, buf: &mut [Sample]) -> usize {
            self.underlying.pop_slice(buf)
        }
    }

    impl Drop for StreamConsumer {
        fn drop(&mut self) {
            self.stream
                .upgrade()
                .expect("stream is upgraded, because it will always exist")
                .remove(self.id)
        }
    }

    impl Debug for StreamConsumer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "(StreamConsumer)")
        }
    }
}
