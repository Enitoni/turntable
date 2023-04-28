use super::config::SAMPLES_PER_SEC;
use ringbuf::{Consumer, Producer, RingBuffer};
use std::{
    io::Read,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

/// Keep track of buffer consumers and remove orphaned ones.
///
/// This is needed because if a buffer isn't consumed and becomes full,
/// the audio processing will stop to accomodate for it. However, since
/// it will never be read from again, this is essentially a deadlock.
pub struct BufferRegistry {
    entries: Mutex<Vec<AudioBufferProducer>>,
}

impl BufferRegistry {
    pub fn new() -> Self {
        Self {
            entries: Default::default(),
        }
    }

    pub fn get_consumer(&self) -> AudioBufferConsumer {
        let mut entries = self.entries.lock().unwrap();

        let buffer = RingBuffer::new(SAMPLES_PER_SEC);
        let (producer, consumer) = buffer.split();

        let consumer = AudioBufferConsumer::new(consumer);
        let state = Arc::downgrade(&consumer.state);

        let producer = AudioBufferProducer::new(producer, state);
        entries.push(producer);

        consumer
    }

    /// Remove dead buffers
    #[allow(dead_code)]
    pub fn recycle(&self) {
        let mut entries = self.entries.lock().unwrap();

        entries.retain(|e| match e.state.upgrade() {
            Some(arc) => {
                let state = arc.lock().unwrap();
                matches!(*state, ProducerState::Alive)
            }
            None => false,
        });
    }

    pub fn write_byte_samples(&self, data: &[u8]) {
        let mut entries = self.entries.lock().unwrap();

        for entry in entries.iter_mut() {
            entry.underlying.push_slice(data);
        }
    }
}

pub enum ProducerState {
    /// The consumer for the buffer is still consuming
    Alive,
    /// The consumer has been dropped and we need to clean this up
    Dead,
}

pub struct AudioBufferProducer {
    state: Weak<Mutex<ProducerState>>,
    underlying: Producer<u8>,
}

impl AudioBufferProducer {
    fn new(underlying: Producer<u8>, state: Weak<Mutex<ProducerState>>) -> Self {
        Self { underlying, state }
    }
}

/// A single entry, created to create a new audio stream consumer
///
/// This struct is a reader into the main audio stream, allowing
/// the stream to be consumed by multiple sources.
pub struct AudioBufferConsumer {
    state: Arc<Mutex<ProducerState>>,
    underlying: Consumer<u8>,
}

impl AudioBufferConsumer {
    pub fn wait_for_buffer(&self, samples_to_wait_for: usize) {
        let seconds_per_sample = 1. / SAMPLES_PER_SEC as f32;
        let seconds_to_wait = (samples_to_wait_for as f32) * seconds_per_sample;

        spin_sleep::sleep(Duration::from_secs_f32(seconds_to_wait));
    }

    fn new(underlying: Consumer<u8>) -> Self {
        Self {
            underlying,
            state: Arc::new(ProducerState::Alive.into()),
        }
    }
}

impl Read for AudioBufferConsumer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let requested_len = buf.len();
        let mut bytes_read = 0;

        while bytes_read < requested_len {
            bytes_read += self.underlying.pop_slice(&mut buf[bytes_read..]);

            if bytes_read < requested_len {
                let remaining = requested_len - bytes_read;

                // Waiting for buffer ensures minimal busy-wait
                self.wait_for_buffer(remaining);
            }
        }

        Ok(requested_len)
    }
}

// Ensure state is updated when this is dropped
impl Drop for AudioBufferConsumer {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        *state = ProducerState::Dead;
    }
}
