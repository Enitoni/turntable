use fundsp::{hacker::AudioNode, oscillator::Sine};
use std::{
    io::{Read, Seek},
    sync::{Arc, Mutex},
};

// Represents a continuous stream of raw PCM audio.
pub struct AudioStream {
    oscillator: Mutex<Sine<f32>>,
}

impl AudioStream {
    pub fn new() -> Self {
        Self {
            oscillator: Mutex::new(Sine::new(44100.)),
        }
    }
}

pub struct AudioStreamRef {
    stream: Arc<AudioStream>,
}

impl AudioStreamRef {
    pub fn new(stream: Arc<AudioStream>) -> Self {
        Self { stream }
    }
}

impl Read for AudioStreamRef {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let samples_to_process = buf.len() / 4;

        let mut sample_buffer = vec![0.; samples_to_process];
        let frequencies = vec![40.; samples_to_process];

        self.stream
            .oscillator
            .lock()
            .expect("Locked oscillator")
            .process(
                samples_to_process,
                &[&frequencies],
                &mut [&mut sample_buffer],
            );

        let mapped_buffer: Vec<u8> = sample_buffer
            .into_iter()
            .flat_map(|sample| sample.to_ne_bytes())
            .collect();

        let len = buf.len().min(mapped_buffer.len());

        buf[..len].copy_from_slice(&mapped_buffer[..len]);
        Ok(len)
    }
}

impl Seek for AudioStreamRef {
    fn seek(&mut self, _: std::io::SeekFrom) -> std::io::Result<u64> {
        panic!("Attempt to seek AudioStream!")
    }
}
