use fundsp::hacker32::*;

use ringbuf::{Consumer, Producer, RingBuffer};

use core::time;
use std::{
    io::{Read, Seek},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use super::{AudioBufferConsumer, BufferRegistry};

pub const PCM_MIME: &str = "audio/pcm;rate=44100;encoding=float;bits=32";
pub const SAMPLE_RATE: usize = 44100;
pub const CHANNEL_COUNT: usize = 2;

// How many samples should be pushed to buffers per iteration
const SAMPLE_BUFFER_SIZE: usize = 4096;

pub const BYTES_PER_SAMPLE: usize = 4 * CHANNEL_COUNT;

// How many bytes should a ring buffer contain
pub const BUFFER_SIZE: usize = SAMPLE_BUFFER_SIZE * BYTES_PER_SAMPLE;

// How many time to wake up the thread during a sleep
const WAKE_UP_DIVISOR: f32 = 3.;

enum StreamState {
    Processing,
    Idle,
}

type ArcMut<T> = Arc<Mutex<T>>;

/// An infinite stream of audio that supports
/// multiple consumers.
pub struct AudioStream {
    signal: ArcMut<Box<dyn AudioUnit32>>,
    state: ArcMut<StreamState>,
    registry: Arc<BufferRegistry>,
}

impl AudioStream {
    pub fn new() -> Self {
        let signal = Self::setup();

        Self {
            signal: Arc::new(signal.into()),
            state: Arc::new(StreamState::Idle.into()),
            registry: BufferRegistry::new().into(),
        }
    }

    /// Start processing the stream.
    /// This will push to ring buffers if state is set to Processing.
    pub fn run(&self) {
        println!("Running AudioStream!");

        let state = self.state.clone();

        // Ensure that processing starts
        *state.lock().unwrap() = StreamState::Processing;

        thread::spawn({
            let registry = Arc::clone(&self.registry);
            let signal = Arc::clone(&self.signal);

            move || {
                // Calculate optimal laziness
                let total_samples = (SAMPLE_BUFFER_SIZE * CHANNEL_COUNT) as f32;
                let seconds_per_sample = 1. / SAMPLE_RATE as f32;

                let optimal_sleep_time = total_samples * seconds_per_sample / WAKE_UP_DIVISOR;
                let time_to_sleep = Duration::from_secs_f32(optimal_sleep_time);

                let mut now = Instant::now();

                loop {
                    thread::sleep(time_to_sleep);
                    let state = state.lock().unwrap();

                    if let StreamState::Idle = *state {
                        // Avoid processing if stream is idle
                        continue;
                    }

                    let mut signal = signal.lock().expect("Signal not poisoned");
                    let remaining = registry.samples_remaining();

                    if remaining < 2 {
                        continue;
                    }

                    let samples: Vec<_> = (0..remaining)
                        .into_iter()
                        .flat_map(|_| {
                            let (left, right) = (*signal).get_stereo();
                            [left, right]
                        })
                        .flat_map(|sample| sample.to_ne_bytes())
                        .collect();

                    // Push the samples into the ring buffers
                    registry.write_byte_samples(&samples);

                    // Ensure dead buffers are removed
                    registry.recycle();
                }
            }
        });
    }

    /// Creates a new AudioStreamSource to read from the stream
    pub fn get_consumer(&self) -> AudioBufferConsumer {
        self.registry.get_consumer()
    }

    pub fn setup() -> Box<dyn AudioUnit32> {
        let distortion = || shape(Shape::ClipTo(-1., 0.9));
        let white = || noise() >> lowpass_hz(100., 1.0);

        let fundamental = 50.;
        let harmonic = |n: f32, v: f32| {
            lfo(move |t| {
                let pitch = sin(t * (n));
                let order = n;

                (fundamental + (pitch * 0.1) * order)
            }) >> sine()
        };

        let harmonics = harmonic(1., 1.)
            + harmonic(2., 0.9)
            + harmonic(3., 0.1)
            + harmonic(5.1, 0.1)
            + harmonic(8., 0.2)
            + harmonic(12., 0.2)
            + harmonic(13.1, 0.2)
            + harmonic(14.1, 0.1)
            + harmonic(19., 0.1)
            + harmonic(20., 0.1);

        let mut signal = (white() + harmonics) >> distortion() >> split() >> reverb_stereo(1., 8.0);

        signal.reset(Some(44100.));
        Box::new(signal)
    }
}
