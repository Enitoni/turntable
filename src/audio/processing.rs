/// Processing implementations for ffmpeg
mod ffmpeg {
    use anyhow::{Context, Result};

    use std::{
        io::Read,
        process::{Child, ChildStdout, Command, Stdio},
    };

    use crate::audio::{BUFFER_SIZE, CHANNEL_COUNT, SAMPLE_RATE};

    /// What should the ffmpeg process do
    pub enum Operation {
        /// Convert to [Sample]
        ToRaw(String),
    }

    impl Operation {
        fn apply(&self, command: &mut Command) {
            match self {
                Operation::ToRaw(input) => self.convert_to_raw(input, command),
            }
        }

        fn convert_to_raw(&self, input: &str, command: &mut Command) {
            command
                .args(["-i", input])
                .args(["-c:a", "pcm_f32le"])
                .args(["-f", "f32le"])
                .args(["-ar", &SAMPLE_RATE.to_string()])
                .args(["-ac", &CHANNEL_COUNT.to_string()])
                .args(["pipe:"])
                .stdout(Stdio::piped());
        }
    }

    /// An ffmpeg process
    struct Process {
        child: Child,
        stdout: ChildStdout,
    }

    impl Process {
        const BUFFER_SIZE: usize = 1024 * 500;
        const CHUNK_SIZE: usize = BUFFER_SIZE / 10;

        fn new(operation: Operation) -> Result<Self> {
            let mut command = Command::new("ffmpeg");
            operation.apply(&mut command);

            let mut process = command.spawn()?;

            let stdout = process
                .stdout
                .take()
                .context("Could not get stdout from process")?;

            let decoder = Self {
                child: process,
                stdout: stdout.into(),
            };

            Ok(decoder)
        }
    }

    impl Read for Process {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let len = buf.len();
            self.stdout.read(&mut buf[..(Self::CHUNK_SIZE.min(len))])
        }
    }
}

/// Various filters and effects for audio
mod dsp {
    use std::ops::Range;

    use log::trace;

    use crate::audio::{
        pipeline::{SampleReader, SamplesRead, Transform},
        Sample, SAMPLES_PER_SEC,
    };

    /// Trims silence at the beginning and end of audio by using a look-ahead buffer.
    pub struct Trimmer<R> {
        reader: R,

        is_at_start: bool,
        is_at_end: bool,

        buffer: Vec<Sample>,
        buffer_length: usize,
        buffer_cursor: usize,
    }

    impl<R> Trimmer<R>
    where
        R: SampleReader,
    {
        // 20 seconds should be good for most tracks
        const BUFFER_SIZE: usize = SAMPLES_PER_SEC * 20;

        fn ensure_lookahead(&mut self) {
            if self.buffer_cursor < self.buffer_length {
                return;
            }

            let result = self.reader.read_samples(&mut self.buffer);

            self.buffer_cursor = 0;
            self.buffer_length = result.amount();
            self.is_at_end = result.is_empty();
        }

        /// Consumes the reader until there is audible content,
        /// then returns the offset skipped.
        fn consume_until_audible(&mut self) -> usize {
            let mut offset = 0;

            loop {
                let index = self
                    .buffer
                    .iter()
                    .take(self.buffer_length)
                    .position(|s| *s != 0.);

                if let Some(index) = index {
                    self.buffer_cursor = index;
                    self.is_at_start = false;

                    return offset + index;
                }

                offset += self.buffer_length;

                // We reached the end, there's no more.
                if self.is_at_end {
                    return offset;
                }

                // No data here, so let's fetch more
                self.ensure_lookahead();
            }
        }

        /// Returns a safe range to read from the buffer
        fn available_range(&self, amount: usize) -> Range<usize> {
            let silence = self.silent_end();
            let real_length = self.buffer_length.min(silence);

            let remaining = real_length - self.buffer_cursor;
            let safe_end = self.buffer_cursor + amount.min(remaining);

            self.buffer_cursor..safe_end
        }

        /// Returns an offset at which silence begins at the end.
        /// If this returns `buffer_length` there is no silence.
        fn silent_end(&self) -> usize {
            let samples_from_end = self
                .buffer
                .iter()
                .take(self.buffer_length)
                .rev()
                .position(|s| *s != 0.);

            if let Some(amount) = samples_from_end {
                self.buffer_length - amount
            } else {
                self.buffer_length
            }
        }
    }

    impl<R> Transform<R> for Trimmer<R>
    where
        R: SampleReader,
    {
        fn pipe(reader: R) -> Self {
            Self {
                reader,

                is_at_start: true,
                is_at_end: false,

                buffer: vec![0.; Self::BUFFER_SIZE],
                buffer_length: 0,
                buffer_cursor: 0,
            }
        }
    }

    impl<R> SampleReader for Trimmer<R>
    where
        R: SampleReader,
    {
        fn read_samples(&mut self, buf: &mut [Sample]) -> SamplesRead {
            self.ensure_lookahead();

            if self.is_at_start {
                let skipped = self.consume_until_audible();
                trace!("Trimmed {} samples of silence from the start", skipped);
            }

            let range = self.available_range(buf.len());

            let samples_read = range.len();
            self.buffer_cursor += samples_read;

            buf[..samples_read].copy_from_slice(&self.buffer[range]);

            if buf.len() > samples_read {
                let skipped = self.buffer_length - self.silent_end();
                trace!("Trimmed {} samples of silence at the end", skipped);

                SamplesRead::Empty(samples_read)
            } else {
                SamplesRead::More(samples_read)
            }
        }
    }

    /// Convenience methods for [SampleReader]
    pub trait DSP: SampleReader + Sized {
        /// Adds a transform that removes silent parts at the start and end
        /// of the audio stream using a look-ahead.
        fn trim_silence(self) -> Trimmer<Self> {
            Trimmer::pipe(self)
        }
    }

    impl<T: SampleReader> DSP for T {}

    #[cfg(test)]
    mod test {
        use super::DSP;
        use crate::audio::{
            pipeline::{IntoSampleReader, SamplesRead},
            util::pipeline::SampleReader,
            SAMPLES_PER_SEC,
        };

        #[test]
        fn trims_short() {
            let start = vec![0_f32; (SAMPLES_PER_SEC as f32 * 1.35) as usize];
            let middle = vec![1., 3., 6., 2., 0., 0., 2., 6.];
            let end = vec![0.; SAMPLES_PER_SEC * 15];

            let mut whole = start;
            whole.extend(&middle);
            whole.extend(&end);

            let mut trimmer = whole.into_sample_reader().trim_silence();

            // Silence was trimmed from the start
            let mut buf = vec![0.; 2];
            let result = trimmer.read_samples(&mut buf);

            assert_eq!(result, SamplesRead::More(2));
            assert_eq!(&buf[..2], &[1., 3.]);

            // Silence is not trimmed from the middle
            let mut buf = vec![0.; 4];
            let result = trimmer.read_samples(&mut buf);

            assert_eq!(result, SamplesRead::More(4));
            assert_eq!(&buf[..4], &[6., 2., 0., 0.]);

            // Silence is trimmed at th eend
            let mut buf = vec![0.; 3];
            let result = trimmer.read_samples(&mut buf);

            assert_eq!(result, SamplesRead::Empty(2));
            assert_eq!(&buf[..2], &[2., 6.]);
        }
    }
}
