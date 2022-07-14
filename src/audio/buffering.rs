use std::{
    fmt::Debug,
    io::Read,
    ops::Range,
    sync::{Arc, Mutex, Weak},
    thread,
    time::Duration,
};

use ringbuf::{Consumer, Producer, RingBuffer};

use crate::util::merge_ranges;

use super::{Sample, BUFFER_SIZE, BYTES_PER_SAMPLE, CHANNEL_COUNT, SAMPLE_RATE};

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

        let remaining = entries
            .iter()
            .map(|p| p.underlying.remaining())
            .min()
            .unwrap_or(0);

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
    const LOOK_AHEAD_MS: usize = 10;

    pub fn wait_for_buffer(&self) {
        let samples = self.samples();

        let sample_per_sec = SAMPLE_RATE * CHANNEL_COUNT;
        let duration_of_samples_ms = samples * sample_per_sec / 1000;

        if duration_of_samples_ms < Self::LOOK_AHEAD_MS {
            thread::sleep(Duration::from_millis(Self::LOOK_AHEAD_MS as u64));
        }
    }

    fn samples(&self) -> usize {
        self.underlying.len() / 4
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

/// A buffer of concatenated audio
pub struct DynamicBuffer<Id> {
    samples: Mutex<Vec<Sample>>,
    allocations: Mutex<Vec<DynamicBufferAllocation<Id>>>,
}

#[derive(Debug, Clone)]
pub struct DynamicBufferAllocation<Id> {
    id: Id,
    len: usize,
    offset: usize,
    sample_ranges: Vec<Range<usize>>,
}

#[derive(Debug, PartialEq)]
pub enum ReadBufferSamplesResult {
    All {
        samples_read: usize,
    },
    Partial {
        samples_read: usize,
        skip_offset: usize,
    },
    End {
        samples_read: usize,
    },
}

impl<Id> DynamicBuffer<Id>
where
    Id: Clone + PartialEq + Debug,
{
    /// 30 minutes of audio
    const INITIAL_BUFFER_LENGTH: usize = SAMPLE_RATE * CHANNEL_COUNT * 60 * 30;

    pub fn new() -> Self {
        Self {
            samples: vec![0f32; Self::INITIAL_BUFFER_LENGTH].into(),
            allocations: Default::default(),
        }
    }

    pub fn allocate(&self, id: Id, len: usize) {
        let offset = self.len();
        self.resize_if_necessary(len * 2);

        let new_allocation = DynamicBufferAllocation::new(id, offset, len);

        let mut allocations = self.allocations.lock().unwrap();
        allocations.push(new_allocation);
    }

    /// Writes samples to an allocation
    pub fn write_samples(&self, id: Id, local_offset: usize, buf: &[Sample]) {
        let mut allocations = self.allocations.lock().unwrap();
        let mut samples = self.samples.lock().unwrap();

        let allocation = allocations
            .iter_mut()
            .find(|a| a.id == id)
            .expect("Allocation exists by id");

        let range = {
            let start = local_offset;
            let end = local_offset + buf.len();

            allocation.submit_range(start..end)
        };

        samples[range].copy_from_slice(buf);
    }

    pub fn read_samples(&self, offset: usize, buf: &mut [Sample]) -> ReadBufferSamplesResult {
        let requested_samples = buf.len();
        let range = offset..offset + requested_samples;

        let samples = self.samples.lock().unwrap();
        let mut allocations = self.allocations_by_range(range);

        // The absolute offset to skip an allocation
        let mut skip_offset = 0;
        let mut samples_read = 0;

        while let Some(alloc) = allocations.pop() {
            let alloc_range = {
                let start = offset + samples_read;
                let end = start + buf.len();

                alloc.clamped_range(start..end)
            };

            let buf_slice = &mut buf[..alloc_range.len()];

            samples_read += alloc_range.len();
            skip_offset = alloc.offset;

            let not_satisfied = samples_read != requested_samples;
            let is_partial = !alloc.is_end(alloc_range.end) && not_satisfied;

            buf_slice.copy_from_slice(&samples[alloc_range]);

            // The allocation range stops before its end, and we need more samples,
            // so we cannot continue as the read must be continuous.
            if is_partial {
                break;
            }
        }

        if samples_read == buf.len() {
            return ReadBufferSamplesResult::All { samples_read };
        }

        // We are at the end of the buffer (no more allocations)
        if allocations.is_empty() {
            return ReadBufferSamplesResult::End { samples_read };
        }

        ReadBufferSamplesResult::Partial {
            samples_read,
            skip_offset,
        }
    }

    pub fn len(&self) -> usize {
        let allocations = self.allocations.lock().unwrap();
        allocations.iter().fold(0, |acc, x| acc + x.len)
    }

    pub fn id_at_offset(&self, offset: usize) -> Option<Id> {
        let allocations = self.allocations.lock().unwrap();

        allocations
            .iter()
            .find_map(|a| (offset > a.offset && offset < a.end()).then(|| a.id.clone()))
    }

    pub fn empty_ranges_at(&self, _range: Range<usize>) -> Vec<(Id, Range<usize>)> {
        let _allocations = self.allocations.lock().unwrap();

        todo!()
    }

    fn allocations_by_range(&self, range: Range<usize>) -> Vec<DynamicBufferAllocation<Id>> {
        let allocations = self.allocations.lock().unwrap();

        allocations
            .iter()
            .filter(|a| {
                let start_is_within = range.start > a.offset && range.start < a.end();
                let end_is_within = range.end > a.offset && range.end < a.end();

                start_is_within || end_is_within
            })
            .cloned()
            .collect()
    }

    fn resize_if_necessary(&self, new_len: usize) {
        let mut samples = self.samples.lock().unwrap();
        let needs_reallocation = new_len > samples.len();

        if needs_reallocation {
            let new_length = samples.len() + new_len;
            samples.resize(new_length, 0.);
        }
    }
}

impl<Id> DynamicBufferAllocation<Id> {
    fn new(id: Id, offset: usize, len: usize) -> Self {
        Self {
            id,
            len,
            offset,
            sample_ranges: Default::default(),
        }
    }

    pub fn submit_range(&mut self, relative_range: Range<usize>) -> Range<usize> {
        let absolute = self.absolute_range(relative_range.clone());
        let ranges = &mut self.sample_ranges;

        self.sample_ranges = merge_ranges({
            ranges.push(relative_range);
            ranges.to_vec()
        });

        absolute
    }

    /// Returns the clamped range according to the samples available
    fn clamped_range(&self, absolute_range: Range<usize>) -> Range<usize> {
        let offset = self.relative_offset(absolute_range.start);

        let sample_range = self
            .sample_ranges
            .iter()
            .find(|r| r.contains(&offset))
            .cloned()
            .unwrap_or(0..0);

        let new_start = absolute_range.start + sample_range.start;
        let remaining = self
            .len
            .checked_sub(offset - sample_range.start)
            .unwrap_or_default();

        let new_end = new_start + remaining.min(absolute_range.len());

        new_start..new_end
    }

    fn absolute_range(&self, relative_range: Range<usize>) -> Range<usize> {
        let start = self.offset + relative_range.start;
        let end = self.offset + relative_range.end;

        debug_assert!(
            end <= self.end(),
            "Range end does not exceed allocation range"
        );

        start..end
    }

    fn is_end(&self, offset: usize) -> bool {
        offset >= self.len
    }

    fn end(&self) -> usize {
        self.offset + self.len
    }

    fn relative_offset(&self, offset: usize) -> usize {
        offset.checked_sub(self.offset).unwrap_or_default()
    }

    fn relative_range(&self, absolute_range: Range<usize>) -> Range<usize> {
        let start = self.relative_offset(absolute_range.start);
        let end = (start + absolute_range.len()).min(self.len);

        start..end
    }
}

#[cfg(test)]
mod test {
    use super::{DynamicBuffer, ReadBufferSamplesResult};

    #[test]
    fn dynamic_buffers_are_read_correctly_at_end() {
        let buffer = DynamicBuffer::new();

        buffer.allocate(1, 20);
        buffer.write_samples(1, 0, &[0.; 20]);

        buffer.allocate(2, 30);
        buffer.write_samples(2, 0, &[1.; 30]);

        let mut buf = vec![0.; 10];
        let amount = buffer.read_samples(45, &mut buf);

        assert_eq!(amount, ReadBufferSamplesResult::End { samples_read: 5 });
    }
}
