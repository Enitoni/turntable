use std::{
    io::Read,
    sync::{Arc, Mutex, Weak},
};

use ringbuf::{Consumer, Producer, RingBuffer};

use super::{BUFFER_SIZE, BYTES_PER_SAMPLE};

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

        let buffer = RingBuffer::new(BUFFER_SIZE);
        let (producer, consumer) = buffer.split();

        let consumer = AudioBufferConsumer::new(consumer);
        let state = Arc::downgrade(&consumer.state);

        let producer = AudioBufferProducer::new(producer, state);
        entries.push(producer);

        consumer
    }

    /// Remove dead buffers
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

    /// Returns how many samples can be pushed before
    /// one of the buffers will be full
    pub fn samples_remaining(&self) -> usize {
        let entries = self.entries.lock().unwrap();

        //dbg!(entries.len());

        let remaining = entries
            .iter()
            .map(|p| p.underlying.remaining())
            .min()
            .unwrap_or(BUFFER_SIZE);

        remaining / BYTES_PER_SAMPLE
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

        let mut read_len = 0;

        while read_len < requested_len {
            let slice = &mut buf[read_len..];
            let bytes_read = self.underlying.read(slice);

            match bytes_read {
                Ok(len) => read_len += len,
                Err(_) => continue,
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
