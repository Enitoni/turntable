use fundsp::{hacker32::*, oscillator::Sine};

use std::{
    io::{Read, Seek},
    sync::{Arc, Mutex},
};

// Represents a continuous stream of raw PCM audio.
pub struct AudioStream {
    signal: Mutex<Box<dyn AudioUnit32>>,
}

impl AudioStream {
    pub fn new() -> Self {
        let signal = Self::setup();

        Self {
            signal: Mutex::new(signal),
        }
    }

    pub fn setup() -> Box<dyn AudioUnit32> {
        let white = || noise() >> (lowpass_hz(100., 1.0) * 0.5);

        let fundamental = 50.;
        let harmonic = |n: f32, v: f32| sine_hz(fundamental * n) * v;

        let harmonics = harmonic(1., 1.)
            + harmonic(2., 0.9)
            + harmonic(3., 0.1)
            + harmonic(5.1, 0.1)
            + harmonic(8., 0.2)
            + harmonic(12., 0.2)
            + harmonic(13.1, 0.2)
            + harmonic(14.1, 0.1);

        let mut signal = (white() + harmonics * 0.3) >> split() >> reverb_stereo(1., 8.0);

        signal.reset(Some(44100.));
        Box::new(signal)
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
        let buffer_length = buf.len();
        let samples_to_process = buffer_length / 4 / 2;

        let mut signal = self.stream.signal.lock().expect("Locked oscillator");

        let samples_as_bytes: Vec<u8> = (0..samples_to_process)
            .into_iter()
            .flat_map(|_| {
                let (left, right) = signal.get_stereo();

                [left.to_ne_bytes(), right.to_ne_bytes()]
                    .into_iter()
                    .flatten()
            })
            .collect();

        buf.copy_from_slice(&samples_as_bytes);
        Ok(buffer_length)
    }
}

impl Seek for AudioStreamRef {
    fn seek(&mut self, _: std::io::SeekFrom) -> std::io::Result<u64> {
        panic!("Attempt to seek AudioStream!")
    }
}
