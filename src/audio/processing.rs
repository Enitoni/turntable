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
