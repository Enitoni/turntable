use std::{
    collections::VecDeque,
    ops::{Add, Mul},
};

use crate::{Config, Sample, VecDequeExt};

use super::Introspect;

/// A buffer that stores a single range of data. Used in [MultiRangeBuffer].
#[derive(Debug)]
pub struct RangeBuffer {
    /// The offset in samples of the start of the buffer.
    offset: usize,
    /// The samples in the buffer.
    data: VecDeque<Sample>,
}

impl RangeBuffer {
    fn new(offset: usize) -> Self {
        Self {
            offset,
            data: Default::default(),
        }
    }

    fn write(&mut self, buf: &[Sample]) {
        self.data.extend(buf)
    }

    /// Reads samples to the provided slice at the given absolute offset.
    fn read(&self, offset: usize, buf: &mut [Sample]) -> usize {
        let available_amount_at_offset = self
            .data
            .len()
            .saturating_sub(offset.saturating_sub(self.offset));

        let requested_amount = buf.len();

        let absolute_start = self.offset;
        let absolute_end = absolute_start + self.data.len();

        assert!(
            offset <= absolute_end,
            "attempt to read from RangeBuffer at offset beyond end"
        );

        let relative_start = offset.saturating_sub(absolute_start);
        let relative_end =
            relative_start.saturating_add(requested_amount.min(available_amount_at_offset));

        let amount_to_read = relative_end - relative_start;
        let (first, second) = self.data.range_as_slices(relative_start..relative_end);

        buf[..first.len()].copy_from_slice(first);
        buf[first.len()..first.len() + second.len()].copy_from_slice(second);

        amount_to_read
    }

    /// Clears all samples outside the given window.
    /// End and start are clamped to chunk_size's start and end.
    fn retain_range(&mut self, start: usize, end: usize, chunk_size: usize) {
        let total_length = self.data.len();

        let absolute_start = self.offset;
        let absolute_end = (absolute_start + total_length).saturating_sub(1);

        let absolute_chunk_start = (absolute_start + 1).saturating_div(chunk_size);
        let absolute_chunk_end =
            (absolute_end.add(1).saturating_sub(chunk_size)).saturating_div(chunk_size);

        let safe_start = start.saturating_div(chunk_size).max(absolute_chunk_start) * chunk_size;
        let safe_end = end
            .saturating_div(chunk_size)
            .min(absolute_chunk_end)
            .mul(chunk_size)
            .saturating_add(chunk_size.saturating_sub(1));

        let relative_start = safe_start.saturating_sub(absolute_start);
        let relative_end = safe_end.saturating_sub(absolute_start);

        drop(self.data.drain(..relative_start));
        self.data.truncate(relative_end - relative_start + 1);

        self.offset = relative_start + absolute_start;
    }

    /// Returns the amount of samples in the buffer so far.
    fn length(&self) -> usize {
        self.data.len()
    }

    /// Returns the start and end of the range.
    fn range(&self) -> (usize, usize) {
        let offset = self.offset;

        (offset, offset + self.length().saturating_sub(1))
    }

    /// Returns true if the given offset is within the range of this buffer.
    fn is_within(&self, offset: usize) -> bool {
        let (start, end) = self.range();
        offset >= start && offset <= end
    }

    /// Returns true if the given range is intersecting or adjacent to this range.
    fn is_intersecting_or_adjacent(&self, other: &Self) -> bool {
        let (start, end) = self.range();
        let (other_start, other_end) = other.range();

        other_start.saturating_sub(1) <= end && start.saturating_sub(1) <= other_end
    }

    /// Merges two intersecting or adjacent ranges, then returns the new merged range.
    fn merge_with(self, other: Self) -> Self {
        let (mut first, second) = if self.offset < other.offset {
            (self, other)
        } else {
            (other, self)
        };

        let (start, end) = first.range();
        let (other_start, _) = second.range();

        let intersection = (end + 1).saturating_sub(other_start);

        first
            .data
            .truncate(first.data.len().saturating_sub(intersection));

        first.data.extend(&second.data);

        Self {
            offset: start,
            data: first.data,
        }
    }

    #[cfg(test)]
    fn consume_to_vec(&mut self) -> Vec<Sample> {
        self.data.drain(..).collect()
    }
}

/// A buffer that stores multiple ranges of [Sample].
/// This is needed for seeking, because it has to be possible to write and read samples at any offset.
#[derive(Debug)]
pub struct MultiRangeBuffer {
    ranges: Vec<RangeBuffer>,
    /// The amount of samples that is expected to be written to the buffer.
    expected_length: Option<usize>,
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
pub struct BufferRead {
    /// The amount of samples read.
    pub amount: usize,
    /// The end of the read operation.
    pub end: BufferReadEnd,
}

#[derive(Debug, Clone, Copy)]
pub struct BufferVoidDistance {
    /// The distance from the void in the buffer.
    pub distance: usize,
    /// Determines if the void is the end of the buffer.
    pub is_end: bool,
}

impl MultiRangeBuffer {
    pub fn new(expected_length: Option<usize>) -> Self {
        Self {
            ranges: Default::default(),
            expected_length,
        }
    }

    /// Writes samples to the buffer at the given offset, creating a new range if necessary.
    pub fn write(&mut self, offset: usize, buf: &[Sample]) {
        let mut ranges: Vec<_> = self.ranges.drain(..).collect();

        let range = ranges.iter_mut().find(|x| x.offset == offset);

        if let Some(range) = range {
            range.write(buf);
        } else {
            ranges.push(RangeBuffer::new(offset));
            ranges.last_mut().unwrap().write(buf);
        }

        self.ranges = Self::merge_ranges(ranges);
    }

    /// Reads samples from the buffer at the given offset. Returns a [BufferReadResult], which describes the result of the read operation.
    /// - If the range has more data after the requested amount, the end is set to `More`
    /// - If the range has a gap after the requested amount, or there isn't any range at all, the end is set to `Gap`
    /// - If the requested amount is larger or equal to the expected size, the end is set to `End`
    pub fn read(&self, offset: usize, buf: &mut [Sample]) -> BufferRead {
        let end_offset = offset + buf.len();
        let range = self.ranges.iter().find(|x| x.is_within(offset));

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

        if self.expected_length.is_some() && Some(end_offset) >= self.expected_length {
            end = BufferReadEnd::End;
        }

        BufferRead {
            amount: amount_read,
            end,
        }
    }

    /// Returns the distance in samples from the offset to the first gap or end of the buffer.
    pub fn distance_from_void(&self, offset: usize) -> BufferVoidDistance {
        let range = self
            .ranges
            .iter()
            .enumerate()
            .find(|(_, x)| x.is_within(offset));

        if let Some((i, range)) = range {
            let has_more = self.ranges.get(i + 1).is_some();
            let (_, end) = range.range();

            BufferVoidDistance {
                distance: end + 1 - offset,
                is_end: !has_more,
            }
        } else {
            BufferVoidDistance {
                distance: 0,
                // This should be ignored, we're at the void no matter what.
                is_end: false,
            }
        }
    }

    /// Returns the distance in samples from the offset to the end of the buffer. If the end is unknown, this number is usize::MAX.
    pub fn distance_from_end(&self, offset: usize) -> usize {
        self.expected_length
            .unwrap_or(usize::MAX)
            .saturating_sub(offset)
    }

    /// Clears all samples outside the given window.
    pub fn retain_window(&mut self, offset: usize, window: usize, chunk_size: usize) {
        let mut ranges: Vec<_> = self.ranges.drain(..).collect();

        let start = offset.saturating_sub(window);
        let end = offset + window;

        ranges.retain(|x| x.is_within(start) || x.is_within(end));

        for range in ranges.iter_mut() {
            range.retain_range(start, end, chunk_size);
        }

        self.ranges = Self::merge_ranges(ranges);
    }

    /// Returns the expected length in samples. If length is [None], it is unknown.
    pub fn expected_length(&self) -> Option<usize> {
        self.expected_length
    }

    /// Merges all ranges that are intersecting or adjacent to each other.
    fn merge_ranges(mut ranges: Vec<RangeBuffer>) -> Vec<RangeBuffer> {
        // Avoid a panic caused by the remove(0) call later on.
        if ranges.is_empty() {
            return vec![];
        }

        ranges.sort_by(|a, b| a.offset.cmp(&b.offset));

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
        merged_ranges
    }

    #[cfg(test)]
    fn consume_to_vec(&mut self) -> Vec<Vec<Sample>> {
        self.ranges
            .drain(..)
            .map(|mut x| x.consume_to_vec())
            .collect()
    }
}

#[derive(Debug)]
pub struct RangeInstrospection {
    pub offset: usize,
    pub length: usize,
}

#[derive(Debug)]
pub struct MultiRangeBufferIntrospection {
    pub ranges: Vec<RangeInstrospection>,
    pub expected_length: Option<usize>,
    /// The size of the buffer in bytes
    pub current_size: usize,
}

impl Introspect<RangeInstrospection> for RangeBuffer {
    fn introspect(&self) -> RangeInstrospection {
        RangeInstrospection {
            offset: self.offset,
            length: self.length(),
        }
    }
}

impl Introspect<MultiRangeBufferIntrospection> for MultiRangeBuffer {
    fn introspect(&self) -> MultiRangeBufferIntrospection {
        let ranges = self.ranges.introspect();
        let current_size: usize = ranges
            .iter()
            .map(|r| r.length * Config::SAMPLES_IN_BYTES)
            .sum();

        MultiRangeBufferIntrospection {
            ranges,
            current_size,
            expected_length: self.expected_length,
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{assign_slice, assign_slice_with_offset};

    use super::*;

    #[test]
    fn test_intersecting_or_adjacent() {
        let mut first = RangeBuffer::new(0);
        let mut second = RangeBuffer::new(5);

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
        let mut first = RangeBuffer::new(0);
        let mut second = RangeBuffer::new(6);

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

        let mut first = RangeBuffer::new(0);
        let mut second = RangeBuffer::new(5);

        first.write(&[1., 2., 3., 4., 5.]);
        second.write(&[6., 7., 8., 9., 10.]);

        let mut merged = first.merge_with(second);
        assert_eq!(
            merged.consume_to_vec(),
            end_result,
            "first should be merged with second"
        );

        let mut first = RangeBuffer::new(0);
        let mut second = RangeBuffer::new(5);

        first.write(&[1., 2., 3., 4., 5.]);
        second.write(&[6., 7., 8., 9., 10.]);

        let mut merged = second.merge_with(first);
        assert_eq!(
            merged.consume_to_vec(),
            end_result,
            "second should be merged with first"
        );

        // Check with an intersecting range
        let mut first = RangeBuffer::new(0);
        let mut second = RangeBuffer::new(5);

        first.write(&[1., 2., 3., 4., 5., 6., 7.]);
        second.write(&[6., 7., 8., 9., 10.]);

        let mut merged = second.merge_with(first);
        assert_eq!(
            merged.consume_to_vec(),
            end_result,
            "second should be merged with first with intersection of 2"
        );
    }

    #[test]
    fn test_retain_range() {
        // Two channels, L and R.
        // Note that this could be any number of channels, but we're only testing with two.
        let chunk_size = 2;
        let absolute_offset = 6;

        // Create the buffer at the specified offset.
        let mut buffer = RangeBuffer::new(absolute_offset);

        // Write some initial samples
        // Since we start at 6, which is an even offset (odd since arrays starts at 0), the first sample is left channel, followed by right channel, and so on.
        // In other words: Odd offsets are L, even offsets are R, if arrays were to start at 1.
        // For convenience, the "samples" (numbers we're using here) follow this pattern:
        //             L   R   L   R   L   R   L   R   L   R
        buffer.write(&[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.]);

        let get_samples = |buffer: &RangeBuffer| {
            let mut buf = vec![0.; 2];

            // Get the fifth and sixth sample
            buffer.read(absolute_offset + 4, &mut buf);
            buf
        };

        let expected_samples = get_samples(&buffer);
        assert_eq!(&expected_samples, &[5., 6.], "sample is correct");

        // Define our offsets. Odd/and even are swapped because arrays start at 0.
        // Specifies sample 4. (even, R). Should get normalized to 3. (odd, L).
        let odd_start_offset = absolute_offset + 3;
        // Specifies sample 7. (odd, L). Should get normalized to 8. (even, R).
        let even_end_offset = absolute_offset + 6;

        // Test the function.
        buffer.retain_range(odd_start_offset, even_end_offset, chunk_size);

        // A channel shift due to an incorrect new absolute offset should not happen, so we get the same samples.
        assert_eq!(
            get_samples(&buffer),
            expected_samples,
            "received samples are correct"
        );
        // New absolute offset should be rounded down to the odd (L) sample 3.
        assert_eq!(
            buffer.offset,
            odd_start_offset - 1,
            "offset is changed accordingly"
        );
        // The remaining samples must follow the pattern, so first is odd, last is even.
        assert_eq!(
            buffer.consume_to_vec(),
            //   L   R   L   R   L   R
            vec![3., 4., 5., 6., 7., 8.],
            "samples within window are retained"
        );

        // Check the overflowing, and check that it handles off-by-one offset.
        let absolute_offset = absolute_offset + 1;
        let mut buffer = RangeBuffer::new(absolute_offset);

        // Samples are shifted a channel because we added one to the offset.
        //             R   L   R   L   R   L   R   L   R   L
        buffer.write(&[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.]);

        let start = absolute_offset - 1;
        let end = absolute_offset + 10;

        buffer.retain_range(start, end, chunk_size);

        assert_eq!(
            buffer.offset,
            absolute_offset + 1,
            "offset is changed accordingly"
        );
        assert_eq!(
            buffer.consume_to_vec(),
            //   L   R   L   R   L   R   L   R
            vec![2., 3., 4., 5., 6., 7., 8., 9.],
            "overflowing end is handled correctly"
        );
    }

    #[test]
    fn test_is_within() {
        let mut buffer = RangeBuffer::new(0);
        buffer.write(&[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.]);

        // Remember, offset starts at 0.
        assert!(buffer.is_within(0), "start is within");
        assert!(buffer.is_within(4), "middle is within");
        assert!(buffer.is_within(9), "end is within");
    }

    #[test]
    fn test_merge_multi_ranges() {
        let mut buffer = MultiRangeBuffer::new(None);

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
        let offset = 20;
        let mut buffer = RangeBuffer::new(offset);

        buffer.write(&[1., 2., 3., 4., 5.]);

        // Read the 5 written samples
        let mut buf = vec![0.; 10];
        let amount = buffer.read(offset, &mut buf);

        assert_eq!(&buf[..amount], &[1., 2., 3., 4., 5.], "buf is correct");
        assert_eq!(amount, 5, "amount read is correct");

        // Read at offset
        let mut buf = vec![0.; 10];
        let amount = buffer.read(offset + 3, &mut buf);

        assert_eq!(&buf[..amount], &[4., 5.], "buf is correct");
        assert_eq!(amount, 2, "amount read is correct");
    }

    #[test]
    fn test_read_multi() {
        let mut buffer = MultiRangeBuffer::new(Some(29));

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
        let mut buffer = MultiRangeBuffer::new(None);

        buffer.write(0, &[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.]);
        buffer.write(20, &[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.]);

        let in_middle = buffer.distance_from_void(0);
        let at_end = buffer.distance_from_void(25);

        assert_eq!(
            in_middle.distance, 10,
            "distance from void in first range is correct"
        );
        assert!(!in_middle.is_end, "void after first range is not end");

        assert_eq!(
            at_end.distance, 5,
            "distance from void in second range is correct"
        );
        assert!(
            at_end.is_end,
            "void after second range is the end/last void"
        );

        let at_last_sample = buffer.distance_from_void(29);
        assert!(at_last_sample.is_end, "void after last sample is the end");
    }

    #[test]
    fn test_retain_window() {
        let mut buffer = MultiRangeBuffer::new(None);

        //                L   R   L   R   L   R   L   R   L   R
        buffer.write(0, &[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.]);
        //                 R    L    R    L    R    L    R    L    R    L
        buffer.write(11, &[20., 21., 22., 23., 24., 25., 26., 27., 28., 29.]);

        // 10 is a gap
        buffer.retain_window(10, 3, 2);

        assert_eq!(
            buffer.consume_to_vec(),
            //        L   R   L   R          L    R
            vec![vec![7., 8., 9., 10.], vec![21., 22.]],
            "ranges are correctly retained"
        );
    }

    #[test]
    fn test_assign_slice() {
        let mut buf = vec![0.; 10];

        let amount = assign_slice(&[1., 2., 3., 4.], &mut buf);

        assert_eq!(amount, 4, "amount is correct");
        assert_eq!(
            buf,
            &[1.0, 2.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,],
            "buf is assigned correctly"
        );
    }

    #[test]
    fn test_assign_slice_with_offset() {
        let mut buf = vec![0.; 10];

        let amount = assign_slice_with_offset(2, &[1., 2., 3., 4.], &mut buf);
        assert_eq!(amount, 4, "amount is correct");

        let amount = assign_slice_with_offset(6, &[1., 2., 3., 4., 5.], &mut buf);
        assert_eq!(amount, 4, "amount is correct");

        let amount = assign_slice_with_offset(10, &[1., 2., 3.], &mut buf);
        assert_eq!(amount, 0, "amount is correct");

        assert_eq!(
            &buf,
            &[0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0,],
            "buf is assigned correctly"
        );
    }

    #[test]
    fn test_range_as_slices() {
        let mut deque = VecDeque::new();

        deque.push_back(0);
        deque.push_back(1);
        deque.push_back(2);

        assert_eq!(deque.range_as_slices(1..), (&[1, 2][..], &[][..]));
        assert_eq!(deque.range_as_slices(1..2), (&[1][..], &[][..]));

        deque.push_front(-1);
        deque.push_front(-2);

        assert_eq!(deque.range_as_slices(1..), (&[-1][..], &[0, 1, 2][..]));
        assert_eq!(deque.range_as_slices(1..4), (&[-1][..], &[0, 1][..]));
        assert_eq!(deque.range_as_slices(2..), (&[][..], &[0, 1, 2][..]));
        assert_eq!(deque.range_as_slices(..2), (&[-2, -1][..], &[][..]));
    }
}
