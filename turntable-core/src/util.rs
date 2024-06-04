use std::{marker::PhantomData, vec};

use crossbeam::atomic::AtomicCell;
use parking_lot::RwLock;

use crate::Sample;

pub static ID_COUNTER: AtomicCell<u64> = AtomicCell::new(1);

/// A unique identifier for any type.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Id<T> {
    value: u64,
    kind: PhantomData<T>,
}

impl<T> Id<T> {
    /// Creates a new id.
    pub fn new() -> Self {
        Self {
            value: ID_COUNTER.fetch_add(1),
            kind: PhantomData,
        }
    }

    /// Returns an empty id.
    pub fn none() -> Self {
        Self {
            value: 0,
            kind: PhantomData,
        }
    }
}

impl<T> Default for Id<T> {
    fn default() -> Self {
        Self::none()
    }
}

/// A buffer that stores a single range of data. Used in [MultiRangeBuffer].
#[derive(Debug)]
struct RangeBuffer {
    /// The offset in samples of the start of the buffer.
    offset: AtomicCell<usize>,
    /// The samples in the buffer.
    data: RwLock<Vec<Sample>>,
}

impl RangeBuffer {
    fn new(offset: usize) -> Self {
        Self {
            offset: AtomicCell::new(offset),
            data: Default::default(),
        }
    }

    fn write(&self, buf: &[Sample]) {
        let mut data = self.data.write();
        data.extend_from_slice(buf);
    }

    /// Reads samples to the provided slice at the given absolute offset.
    fn read(&self, offset: usize, buf: &mut [Sample]) -> usize {
        let data = self.data.read();

        let start = offset.saturating_sub(self.offset.load()).min(data.len());
        let end = start.saturating_add(buf.len()).min(data.len());
        let amount = end.saturating_sub(start);

        buf[..amount].copy_from_slice(&data[start..end]);
        amount
    }

    /// Clears all samples outside the given window.
    fn retain_range(&self, start: usize, end: usize) {
        let mut data = self.data.write();

        let offset = self.offset.load();
        let relative_start = start.saturating_sub(offset);
        let relative_end = end.saturating_sub(offset).min(data.len().saturating_sub(1));

        let remainder: Vec<_> = data.drain(relative_start..=relative_end).collect();

        *data = remainder;
        self.offset.store(start.max(offset));
    }

    /// Returns the amount of samples in the buffer so far.
    fn length(&self) -> usize {
        self.data.read().len()
    }

    /// Returns the start and end of the range.
    fn range(&self) -> (usize, usize) {
        let offset = self.offset.load();

        (offset, offset + self.length().saturating_sub(1))
    }

    /// Returns true if the given offset is within the range of this buffer.
    fn is_within(&self, offset: usize) -> bool {
        let (start, end) = self.range();
        offset >= start && offset < end
    }

    /// Returns true if the given range is intersecting or adjacent to this range.
    fn is_intersecting_or_adjacent(&self, other: &Self) -> bool {
        let (start, end) = self.range();
        let (other_start, other_end) = other.range();

        other_start.saturating_sub(1) <= end && start.saturating_sub(1) <= other_end
    }

    /// Merges two intersecting or adjacent ranges, then returns the new merged range.
    fn merge_with(self, other: Self) -> Self {
        let mut new_data = vec![];

        let (first, second) = if self.offset.load() < other.offset.load() {
            (self, other)
        } else {
            (other, self)
        };

        let (start, end) = first.range();
        let (other_start, _) = second.range();

        let first_data: Vec<_> = first.data.write().drain(..).collect();
        let second_data: Vec<_> = second.data.write().drain(..).collect();
        let intersection = (end + 1).saturating_sub(other_start);

        new_data.extend_from_slice(&first_data[..first_data.len().saturating_sub(intersection)]);
        new_data.extend_from_slice(&second_data);

        Self {
            offset: AtomicCell::new(start),
            data: RwLock::new(new_data),
        }
    }

    #[cfg(test)]
    fn consume_to_vec(&self) -> Vec<Sample> {
        let mut data = self.data.write();
        data.drain(..).collect()
    }
}

/// A buffer that stores multiple ranges of [Sample].
/// This is needed for seeking, because it has to be possible to write and read samples at any offset.
#[derive(Debug)]
pub struct MultiRangeBuffer {
    ranges: RwLock<Vec<RangeBuffer>>,
    /// The amount of samples that is expected to be written to the buffer.
    expected_size: usize,
}

/// Describes the end of a read operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferReadEnd {
    /// There is more data to read after the requested amount.
    More,
    /// There is a gap between ranges after the requested amount.
    Gap,
    /// The end of the buffer has been reached.
    End,
}

#[derive(Debug, Clone, Copy)]
pub struct BufferReadResult {
    /// The amount of samples read.
    pub amount: usize,
    /// The end of the read operation.
    pub end: BufferReadEnd,
}

impl MultiRangeBuffer {
    pub fn new(expected_size: usize) -> Self {
        Self {
            ranges: Default::default(),
            expected_size,
        }
    }

    /// Writes samples to the buffer at the given offset, creating a new range if necessary.
    pub fn write(&self, offset: usize, buf: &[Sample]) {
        let mut ranges: Vec<_> = self.ranges.write().drain(..).collect();

        let range = ranges.iter_mut().find(|x| x.offset.load() == offset);

        if let Some(range) = range {
            range.write(buf);
        } else {
            ranges.push(RangeBuffer::new(offset));
            ranges.last_mut().unwrap().write(buf);
        }

        self.merge_ranges(ranges);
    }

    /// Reads samples from the buffer at the given offset. Returns a [BufferReadResult], which describes the result of the read operation.
    /// - If the range has more data after the requested amount, the end is set to `More`
    /// - If the range has a gap after the requested amount, or there isn't any range at all, the end is set to `Gap`
    /// - If the requested amount is larger or equal to the expected size, the end is set to `End`
    pub fn read(&self, offset: usize, buf: &mut [Sample]) -> BufferReadResult {
        let ranges = self.ranges.read();

        let end_offset = offset + buf.len();
        let range = ranges.iter().find(|x| x.is_within(offset));

        let mut amount_read = 0;
        let mut end = BufferReadEnd::More;

        if let Some(range) = range {
            amount_read = range.read(offset, buf);

            let (_, range_end) = range.range();
            let remaining = range_end.saturating_sub(end_offset);

            if remaining == 0 {
                end = BufferReadEnd::Gap;
            }
        } else {
            end = BufferReadEnd::Gap;
        }

        if end_offset >= self.expected_size {
            end = BufferReadEnd::End;
        }

        BufferReadResult {
            amount: amount_read,
            end,
        }
    }

    /// Returns the distance in samples from the offset to the first gap or end of the buffer.
    pub fn distance_from_void(&self, offset: usize) -> usize {
        let ranges = self.ranges.read();
        let range = ranges.iter().find(|x| x.is_within(offset));

        if let Some(range) = range {
            let (_, end) = range.range();
            end + 1 - offset
        } else {
            0
        }
    }

    /// Clears all samples outside the given window.
    pub fn retain_window(&self, offset: usize, window: usize) {
        let mut ranges: Vec<_> = self.ranges.write().drain(..).collect();

        let halved_window = window / 2;
        let start = (offset - halved_window).max(0);
        let end = offset + halved_window;

        ranges.retain(|x| x.is_within(start) || x.is_within(end));

        for range in ranges.iter() {
            range.retain_range(start, end);
        }

        self.merge_ranges(ranges);
    }

    /// Merges all ranges that are intersecting or adjacent to each other.
    fn merge_ranges(&self, mut ranges: Vec<RangeBuffer>) {
        // Avoid a panic caused by the remove(0) call later on.
        if ranges.is_empty() {
            return;
        }

        ranges.sort_by(|a, b| a.offset.load().cmp(&b.offset.load()));

        let mut merged_ranges = vec![];
        let mut current_range = ranges.remove(0);

        for range in ranges {
            if current_range.is_intersecting_or_adjacent(&range) {
                current_range = current_range.merge_with(range);
            } else {
                merged_ranges.push(current_range);
                current_range = range;
            }
        }

        merged_ranges.push(current_range);
        *self.ranges.write() = merged_ranges;
    }

    #[cfg(test)]
    fn consume_to_vec(&self) -> Vec<Vec<Sample>> {
        let mut ranges = self.ranges.write();
        ranges.drain(..).map(|x| x.consume_to_vec()).collect()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_intersecting_or_adjacent() {
        let first = RangeBuffer::new(0);
        let second = RangeBuffer::new(5);

        first.write(&[1., 2., 3., 4., 5.]);
        second.write(&[6., 7., 8., 9., 10.]);

        assert!(
            first.is_intersecting_or_adjacent(&second),
            "first should intersect with second"
        );
        assert!(
            second.is_intersecting_or_adjacent(&first),
            "second should intersect with first"
        );

        // With a gap
        let first = RangeBuffer::new(0);
        let second = RangeBuffer::new(6);

        first.write(&[1., 2., 3., 4., 5.]);
        second.write(&[6., 7., 8., 9., 10.]);

        assert!(
            !first.is_intersecting_or_adjacent(&second),
            "first should not intersect with second"
        );
        assert!(
            !second.is_intersecting_or_adjacent(&first),
            "second should not intersect with first"
        );
    }

    #[test]
    fn test_merge_with() {
        let end_result = [1., 2., 3., 4., 5., 6., 7., 8., 9., 10.];

        let first = RangeBuffer::new(0);
        let second = RangeBuffer::new(5);

        first.write(&[1., 2., 3., 4., 5.]);
        second.write(&[6., 7., 8., 9., 10.]);

        let merged = first.merge_with(second);
        assert_eq!(
            merged.consume_to_vec(),
            end_result,
            "first should be merged with second"
        );

        let first = RangeBuffer::new(0);
        let second = RangeBuffer::new(5);

        first.write(&[1., 2., 3., 4., 5.]);
        second.write(&[6., 7., 8., 9., 10.]);

        let merged = second.merge_with(first);
        assert_eq!(
            merged.consume_to_vec(),
            end_result,
            "second should be merged with first"
        );

        // Check with an intersecting range
        let first = RangeBuffer::new(0);
        let second = RangeBuffer::new(5);

        first.write(&[1., 2., 3., 4., 5., 6., 7.]);
        second.write(&[6., 7., 8., 9., 10.]);

        let merged = second.merge_with(first);
        assert_eq!(
            merged.consume_to_vec(),
            end_result,
            "second should be merged with first with intersection of 2"
        );
    }

    #[test]
    fn test_retain_range() {
        let buffer = RangeBuffer::new(20);
        buffer.write(&[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.]);
        buffer.retain_range(22, 25);

        let new_offset = buffer.offset.load();

        assert_eq!(new_offset, 22, "offset is changed accordingly");
        assert_eq!(
            buffer.consume_to_vec(),
            vec![3., 4., 5., 6.],
            "samples within window are retained"
        );

        let buffer = RangeBuffer::new(20);
        buffer.write(&[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.]);
        buffer.retain_range(27, 100);

        assert_eq!(
            buffer.consume_to_vec(),
            vec![8., 9., 10.],
            "overflowing end is handled correctly"
        );
    }

    #[test]
    fn test_merge_multi_ranges() {
        let buffer = MultiRangeBuffer::new(0);

        buffer.write(0, &[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.]);
        buffer.write(5, &[0., 0., 0.]);
        buffer.write(9, &[1., 1., 1.]);
        assert_eq!(
            buffer.consume_to_vec(),
            vec![vec![1., 2., 3., 4., 5., 0., 0., 0.], vec![1., 1., 1.]],
            "ranges are correctly merged"
        );
    }

    #[test]
    fn test_read() {
        let buffer = MultiRangeBuffer::new(29);

        buffer.write(0, &[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.]);
        buffer.write(20, &[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.]);

        // Reading from the first range
        let mut buf = vec![0.; 6];
        let result = buffer.read(7, &mut buf);

        assert_eq!(result.amount, 3, "amount read is correct");
        assert_eq!(result.end, BufferReadEnd::Gap, "result end is correct");
        assert_eq!(buf, vec![8., 9., 10., 0., 0., 0.], "buf is read correctly");

        // Reading from the second range
        let mut buf = vec![0.; 6];
        let result = buffer.read(22, &mut buf);

        assert_eq!(result.amount, 6, "amount read in second range is correct");
        assert_eq!(result.end, BufferReadEnd::More, "result end is correct");
        assert_eq!(
            buf,
            vec![3., 4., 5., 6., 7., 8.],
            "buf is read correctly from second range"
        );

        // Reading to the end of the buffer
        let mut buf = vec![0.; 3];
        let result = buffer.read(27, &mut buf);

        assert_eq!(result.amount, 3, "amount read is correct");
        assert_eq!(result.end, BufferReadEnd::End, "result end is correct");
        assert_eq!(buf, vec![8., 9., 10.], "buf is read correctly");
    }

    #[test]
    fn test_distance_from_void() {
        let buffer = MultiRangeBuffer::new(0);

        buffer.write(0, &[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.]);
        buffer.write(20, &[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.]);

        assert_eq!(
            buffer.distance_from_void(0),
            10,
            "distance from void in first range is correct"
        );
        assert_eq!(
            buffer.distance_from_void(25),
            5,
            "distance from void in second range is correct"
        );
    }

    #[test]
    fn test_retain_window() {
        let buffer = MultiRangeBuffer::new(0);

        buffer.write(0, &[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.]);
        buffer.write(11, &[20., 21., 22., 23., 24., 25., 26., 27., 28., 29.]);

        // 10 is a gap
        buffer.retain_window(10, 6);

        assert_eq!(
            buffer.consume_to_vec(),
            vec![vec![8., 9., 10.], vec![20., 21., 22.]],
            "ranges are correctly retained"
        );
    }
}