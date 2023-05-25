mod decoding;
mod encoding;
mod events;
mod playback;
mod processing;
mod source;
mod timeline;
mod track;
pub mod util;

use crate::ingest;
pub use decoding::raw_samples_from_bytes;
pub use encoding::*;
pub use events::*;
pub use ingest::Input;
pub use playback::*;
pub use timeline::*;
pub use track::Track;

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

    use parking_lot::{Mutex, RwLock};
    use ringbuf::{Consumer, Producer, RingBuffer};

    use crate::util::ID_COUNTER;

    use super::{Sample, SAMPLES_PER_SEC};

    type StreamConsumerId = u64;

    /// Represents a stream of audio that can be consumed from multiple places.
    pub struct Stream {
        me: Weak<Stream>,
        entries: Mutex<Vec<(StreamConsumerId, Producer<Sample>)>>,

        /// Preloaded samples a consumer will be filled with
        preloaded: RwLock<Vec<Sample>>,
    }

    impl Stream {
        const PRELOAD_BUFFER_SIZE: usize = SAMPLES_PER_SEC;

        pub fn new() -> Arc<Self> {
            Arc::new_cyclic(|me| Stream {
                me: me.clone(),
                entries: Default::default(),
                preloaded: Default::default(),
            })
        }

        /// Create a new consumer preloaded with samples
        pub fn consumer(&self) -> StreamConsumer {
            let buffer = RingBuffer::new(SAMPLES_PER_SEC);

            let (mut producer, consumer) = buffer.split();

            let preloaded = self.preloaded.read();
            producer.push_slice(&preloaded);

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

            self.write_preload(buf);
        }

        fn write_preload(&self, buf: &[Sample]) {
            let mut preloaded = self.preloaded.write();

            preloaded.extend_from_slice(buf);
            let overflowing = preloaded.len().saturating_sub(Self::PRELOAD_BUFFER_SIZE);

            if overflowing > 0 {
                preloaded.drain(..overflowing);
            }
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
