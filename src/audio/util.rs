/// Types and structs to streamline pipelining of audio processing
pub mod pipeline {
    use std::{
        io::Read,
        ops::{Deref, DerefMut},
    };

    use crate::audio::{Sample, SAMPLE_IN_BYTES};
    use log::error;

    /// Reads [Sample] into the provided buffer, returning an enum
    /// which describes how many were read and if there are more.
    pub trait SampleReader {
        fn read_samples(&mut self, buf: &mut [Sample]) -> SamplesRead;
    }

    #[derive(Debug, PartialEq)]
    pub enum SamplesRead {
        /// Samples were read successfully.
        /// If `usize` is less than requested, there is more data
        /// but it is not available right now.
        More(usize),
        /// This is the last chunk of samples.
        Empty(usize),
    }

    impl SamplesRead {
        pub fn empty_if(condition: bool, amount: usize) -> Self {
            if condition {
                Self::Empty(amount)
            } else {
                Self::More(amount)
            }
        }

        pub fn merge(&self, other: Self) -> Self {
            self.map(|x| x + other.amount())
        }

        pub fn map<F>(&self, mut f: F) -> Self
        where
            F: FnMut(usize) -> usize,
        {
            match self {
                SamplesRead::More(x) => Self::More(f(*x)),
                SamplesRead::Empty(x) => Self::Empty(f(*x)),
            }
        }

        /// Returns the amount of samples read
        pub fn amount(&self) -> usize {
            match self {
                SamplesRead::More(x) => *x,
                SamplesRead::Empty(x) => *x,
            }
        }

        pub fn is_empty(&self) -> bool {
            matches!(self, Self::Empty(_))
        }
    }

    pub trait IntoSampleReader {
        type Output: SampleReader;

        fn into_sample_reader(self) -> Self::Output;
    }

    /// Writes [Sample] from the provided buffer.
    pub trait SampleWriter {
        fn write_samples(&mut self, buf: &[Sample]);
    }

    /// Transforms the output of a SampleReader by piping it.
    pub trait Transform<R>: SampleReader {
        fn pipe(reader: R) -> Self;
    }

    /// Blanket implementation to get samples from readers
    impl<T: Read> SampleReader for T {
        /// Interprets the data as [Sample].
        fn read_samples(&mut self, buf: &mut [Sample]) -> SamplesRead {
            let mut internal_buf = vec![0; buf.len() * SAMPLE_IN_BYTES];

            match self.read(&mut internal_buf) {
                Ok(read) => {
                    let samples: Vec<Sample> = internal_buf[..read]
                        .chunks_exact(SAMPLE_IN_BYTES)
                        .map(|b| {
                            let arr: [u8; SAMPLE_IN_BYTES] = [b[0], b[1], b[2], b[3]];
                            Sample::from_le_bytes(arr)
                        })
                        .collect();

                    let samples_read = samples.len();
                    buf.copy_from_slice(&samples);

                    if samples_read < buf.len() {
                        SamplesRead::Empty(samples_read)
                    } else {
                        SamplesRead::More(samples_read)
                    }
                }
                Err(err) => match err.kind() {
                    std::io::ErrorKind::Interrupted => SamplesRead::More(0),
                    _ => {
                        error!("Sample conversion failed: {}", err);
                        SamplesRead::Empty(0)
                    }
                },
            }
        }
    }

    /// A vec of samples
    #[derive(Debug)]
    pub struct SampleVec(Vec<Sample>);

    impl SampleReader for SampleVec {
        fn read_samples(&mut self, buf: &mut [Sample]) -> SamplesRead {
            if self.is_empty() {
                return SamplesRead::Empty(0);
            }

            let requested_len = buf.len();

            let safe_len = requested_len.min(self.len());
            let samples: Vec<_> = self.0.drain(..safe_len).collect();

            buf[..samples.len()].copy_from_slice(&samples);
            SamplesRead::empty_if(requested_len > samples.len(), samples.len())
        }
    }

    impl IntoSampleReader for Vec<Sample> {
        type Output = SampleVec;

        fn into_sample_reader(self) -> Self::Output {
            SampleVec(self)
        }
    }

    impl From<Vec<Sample>> for SampleVec {
        fn from(vec: Vec<Sample>) -> Self {
            Self(vec)
        }
    }

    impl Deref for SampleVec {
        type Target = Vec<Sample>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl DerefMut for SampleVec {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    #[cfg(test)]
    mod test {
        use super::{IntoSampleReader, SampleReader, SamplesRead};

        #[test]
        fn sample_vec() {
            let mut samples = vec![1_f32, 2., 5., 7., 8., 9.].into_sample_reader();
            let mut buffer = vec![0.; samples.len()];

            let result = samples.read_samples(&mut buffer[..3]);
            assert_eq!(result, SamplesRead::More(3));

            let result = samples.read_samples(&mut buffer[..6]);
            assert_eq!(result, SamplesRead::Empty(3));
        }
    }
}
