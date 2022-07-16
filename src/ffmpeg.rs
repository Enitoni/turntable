use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io::{Read, Seek, SeekFrom},
    process::{Child, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};

use crossbeam::atomic::AtomicCell;

use crate::audio::{
    self, raw_samples_from_bytes, AudioSource, SourceId, CHANNEL_COUNT, SAMPLES_PER_SEC,
    SAMPLE_IN_BYTES, SAMPLE_RATE,
};

pub enum CreateError {
    SpawnFailed,
}

/// Decodes the input to raw audio supported by [AudioSource]
#[derive(Debug, Clone)]
pub struct RawDecoder {
    stream: Arc<Mutex<ChildStdout>>,
    process: Arc<Child>,
    length: usize,
    input: String,

    buffer: Arc<Mutex<Vec<u8>>>,
    buffer_loaded: Arc<AtomicCell<usize>>,
}

impl RawDecoder {
    const CHUNK_SIZE: usize = (SAMPLES_PER_SEC * 10) * SAMPLE_IN_BYTES;

    fn new<T: ToString>(length: usize, input: T) -> Result<Self, CreateError> {
        let input = input.to_string();

        let mut process = Command::new("ffmpeg")
            .args(["-i", &input])
            .args(["-c:a", "pcm_f32le"])
            .args(["-f", "f32le"])
            .args(["-ar", &SAMPLE_RATE.to_string()])
            .args(["-ac", &CHANNEL_COUNT.to_string()])
            .args(["pipe:"])
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|_| CreateError::SpawnFailed)?;

        let stream = process
            .stdout
            .take()
            .expect("stdout exists in ffmpeg process");

        let mut decoder = Self {
            input,
            length,
            process: process.into(),
            stream: Arc::new(stream.into()),

            buffer: Arc::new(vec![0; length].into()),
            buffer_loaded: Arc::new(0.into()),
        };

        decoder.run_in_thread();

        Ok(decoder)
    }

    fn run_in_thread(&self) {
        thread::spawn({
            let stream = self.stream.clone();
            let buffer = self.buffer.clone();
            let buffer_loaded = self.buffer_loaded.clone();

            move || loop {
                let mut buffer = buffer.lock().unwrap();
                let mut stream = stream.lock().unwrap();

                let start = buffer_loaded.load();
                let end = (start + Self::CHUNK_SIZE).min(buffer.len());

                let read = stream.read(&mut buffer[start..end]).unwrap();
                buffer_loaded.store(start + read);

                if read < Self::CHUNK_SIZE {
                    break;
                }
            }
        });
    }
}

impl AudioSource for RawDecoder {
    fn length(&self) -> usize {
        self.length
    }

    fn id(&self) -> SourceId {
        let mut hasher = DefaultHasher::default();

        let fingerprint = &self.input;
        fingerprint.hash(&mut hasher);

        hasher.finish()
    }

    fn read_samples(
        &mut self,
        offset: usize,
        buf: &mut [audio::Sample],
    ) -> Result<usize, audio::SourceError> {
        let mut samples_loaded = 0;

        loop {
            let buffer = self.buffer.lock().unwrap();
            let buffer_loaded = self.buffer_loaded.load();

            let start = offset + samples_loaded;
            let end = (start + buf.len()).min(buffer_loaded / SAMPLE_IN_BYTES);

            let raw_slice = &buffer[(start * SAMPLE_IN_BYTES)..end];
            let samples = raw_samples_from_bytes(raw_slice);

            buf[samples_loaded..samples.len()].copy_from_slice(&samples);
            samples_loaded += samples.len();

            if samples_loaded == buf.len() {
                break;
            }
        }

        Ok(samples_loaded)
    }
}
