use super::{Sample, SAMPLE_IN_BYTES};

/// Converts a slice of bytes into a vec of [Sample].
pub fn raw_samples_from_bytes(bytes: &[u8]) -> Vec<Sample> {
    bytes
        .chunks_exact(SAMPLE_IN_BYTES)
        .map(|b| {
            let arr: [u8; SAMPLE_IN_BYTES] = [b[0], b[1], b[2], b[3]];
            Sample::from_le_bytes(arr)
        })
        .collect()
}
