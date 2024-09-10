use std::{
    collections::VecDeque,
    ops::{Bound, RangeBounds},
};

pub trait VecDequeExt<T> {
    fn range_as_slices<R: RangeBounds<usize>>(&self, range: R) -> (&[T], &[T]);
}

impl<T> VecDequeExt<T> for VecDeque<T> {
    fn range_as_slices<R: RangeBounds<usize>>(&self, range: R) -> (&[T], &[T]) {
        let start = match range.start_bound() {
            Bound::Included(&i) => i,
            Bound::Excluded(&i) => i + 1,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&i) => i + 1,
            Bound::Excluded(&i) => i,
            Bound::Unbounded => self.len(),
        };

        let (head, tail) = self.as_slices();
        let head_len = head.len();
        let head = &head[start.min(head.len())..end.min(head.len())];
        let tail =
            &tail[start.saturating_sub(head_len)..(end.saturating_sub(head_len)).min(tail.len())];
        (head, tail)
    }
}

/// A utility function to safely assign a slice to a mutable slice.
/// Returns the amount of elements that were assigned.
pub fn assign_slice<T: Copy>(from: &[T], to: &mut [T]) -> usize {
    let safe_end = from.len().min(to.len());
    to[..safe_end].copy_from_slice(&from[..safe_end]);
    safe_end
}

/// Same as `assign_slice`, but with an offset.
/// If the offset is larger than the length of the slice, data is truncated.
pub fn assign_slice_with_offset<T: Copy>(offset: usize, from: &[T], to: &mut [T]) -> usize {
    let offset = offset.min(to.len());
    assign_slice(from, &mut to[offset..])
}
