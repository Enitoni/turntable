mod decoding;
mod encoding;
mod processing;
mod queuing;
mod source;
mod track;
pub mod util;

use crate::ingest;
pub use decoding::raw_samples_from_bytes;
pub use encoding::*;
pub use ingest::Input;
pub use queuing::*;
pub use track::Track;
pub use util::pipeline;

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

    pub const PRELOAD_AMOUNT: usize = 512 * 1000;
    pub const PRELOAD_THRESHOLD: usize = SAMPLES_PER_SEC * 20;
}

pub use config::*;

pub mod new {

    use std::sync::Weak;
    use std::time::Duration;
    use std::{fmt::Debug, sync::Arc};

    use crossbeam::atomic::AtomicCell;
    use parking_lot::{Mutex, RwLock};
    use ringbuf::{Consumer, Producer, RingBuffer};

    use crate::audio::PRELOAD_THRESHOLD;
    use crate::ingest::{Sink, SinkId};
    use crate::util::ID_COUNTER;

    use super::{Sample, SAMPLES_PER_SEC, STREAM_CHUNK_SIZE};

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

            for advancement in advancements {
                amount_read += advancement
                    .sink
                    .read(advancement.start_offset, &mut samples[amount_read..]);
            }

            self.stream.write(&samples);

            let new_sink_offset = self.timeline.offset.load();
            let difference = new_sink_offset.saturating_sub(current_offset);

            ProcessMetadata {
                new_sink_offset,
                consumed_sinks,
                difference,
            }
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
    }

    impl Timeline {
        pub fn set_sinks(&self, sinks: Vec<Sink>) {
            *self.sinks.lock() = sinks
        }

        /// Optionally returns a sink to preload if necessary
        pub fn preload(&self) -> Option<SinkId> {
            let sinks: Vec<_> = self.sinks.lock().iter().cloned().collect();

            let total_available: usize = sinks
                .iter()
                .scan(true, |previous_was_complete, item| {
                    if !*previous_was_complete {
                        return None;
                    }

                    let available = item.available();
                    *previous_was_complete = item.is_complete();

                    Some(available)
                })
                .sum();

            let available = total_available.saturating_sub(self.offset.load());

            if available > PRELOAD_THRESHOLD {
                return None;
            }

            sinks
                .into_iter()
                .filter(|s| !s.is_complete())
                .take(1)
                .find_map(|s| (!s.is_pending()).then(|| s.id()))
        }

        /// Advance the timeline and return a list of advancements describing sinks to read from.
        pub fn advance(&self, amount: usize) -> Vec<Advancement> {
            let mut result = vec![];

            let sinks: Vec<_> = self.sinks.lock().iter().cloned().collect();

            let mut remaining = amount;
            let mut offset = self.offset.load();

            for sink in sinks {
                if remaining == 0 {
                    break;
                }

                let available = sink.available();
                let amount_ahead = available.saturating_sub(offset);
                let amount_to_read = amount_ahead.min(remaining);
                let new_offset = offset + amount_to_read;

                remaining -= amount_to_read;
                result.push(Advancement {
                    sink: sink.clone(),
                    start_offset: offset,
                    end_offset: new_offset,
                });

                self.total_offset.fetch_add(amount_to_read);
                self.offset.store(new_offset);

                offset = 0;

                if !sink.is_complete() {
                    break;
                }
            }

            result
        }
    }

    #[derive(Debug)]
    /// Read from a sink at a specific offset.
    pub struct Advancement {
        sink: Sink,
        start_offset: usize,
        end_offset: usize,
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

        /// Remove an entry after the consumer has been dropped
        ///
        /// **This should not ever be called manually.**
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
        ///
        /// **Note: This will block if the ringbuffer is empty, until it is not**
        pub fn read(&mut self, buf: &mut [Sample]) -> usize {
            let requested_samples = buf.len();
            let mut samples_read = 0;

            while samples_read < requested_samples {
                samples_read += self.underlying.pop_slice(&mut buf[samples_read..]);

                if samples_read < requested_samples {
                    let remaining = requested_samples - samples_read;

                    // Waiting for buffer ensures minimal busy-wait
                    wait_for_buffer(remaining);
                }
            }

            requested_samples
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

    fn wait_for_buffer(samples_to_wait_for: usize) {
        let seconds_per_sample = 1. / SAMPLES_PER_SEC as f32;
        let seconds_to_wait = (samples_to_wait_for as f32) * seconds_per_sample;

        spin_sleep::sleep(Duration::from_secs_f32(seconds_to_wait));
    }
}
