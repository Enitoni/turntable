use std::ops::Range;

/// Returns a safe range that will not overflow
pub fn safe_range(length: usize, range: Range<usize>) -> Range<usize> {
    let start = range.start.min(length - 1);
    let end = range.end.max(start).min(length - 1);

    start..end
}

pub fn merge_ranges<T>(ranges: Vec<Range<T>>) -> Vec<Range<T>>
where
    T: PartialOrd + Ord + Clone,
{
    let mut result = vec![];

    let mut current_range = ranges
        .get(0)
        .cloned()
        .expect("At least one range in the vec");

    for range in ranges.into_iter().skip(1) {
        if current_range.contains(&range.start) {
            current_range.end = current_range.end.max(range.end);
        } else {
            result.push(current_range);
            current_range = range;
        }
    }

    result
}
