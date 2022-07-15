use std::{fmt::Debug, ops::Range};

/// Returns a safe range that will not overflow
pub fn safe_range(length: usize, range: Range<usize>) -> Range<usize> {
    let start = range.start.min(length - 1);
    let end = range.end.max(start).min(length - 1);

    start..end
}

pub fn ranges_overlap<T>(a: &Range<T>, b: &Range<T>) -> bool
where
    T: PartialOrd + Ord + Clone,
{
    a.contains(&b.start) || b.contains(&a.end)
}

pub fn combine_two_ranges<T>(a: &Range<T>, b: &Range<T>) -> Range<T>
where
    T: Copy + PartialOrd + Ord,
{
    let start = a.start.min(b.start);
    let end = a.end.max(b.end);

    start..end
}

pub fn merge_ranges<T>(ranges: Vec<Range<T>>) -> Vec<Range<T>>
where
    T: PartialOrd + Ord + Clone + Copy + Debug,
{
    let mut results = vec![];
    let mut current = ranges.first().cloned().unwrap();

    for (i, range) in ranges.iter().enumerate() {
        if ranges_overlap(&current, range) {
            current = combine_two_ranges(&current, range);
        } else {
            results.push(current);
            current = range.clone();
        }

        if i == ranges.len() - 1 {
            results.push(current.clone());
        }
    }

    results
}

#[cfg(test)]
mod test {
    use super::merge_ranges;

    #[test]
    fn merge_ranges_works() {
        let ranges_to_merge = vec![0..4, 3..7, 7..35, 6..90];
        let merged = merge_ranges(ranges_to_merge);
        assert_eq!(merged, vec![0..90]);

        let ranges_to_merge = vec![0..4, 2..3, 7..35, 6..90];
        let merged = merge_ranges(ranges_to_merge);
        assert_eq!(merged, vec![0..4, 6..90]);
    }
}
