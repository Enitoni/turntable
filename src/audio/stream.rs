use fundsp::hacker32::*;

use ringbuf::{Consumer, Producer, RingBuffer};

use core::time;
use std::{
    io::{Read, Seek},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

pub const SAMPLE_RATE: usize = 44100;
pub const CHANNEL_COUNT: usize = 2;

// How many samples should be pushed to buffers per iteration
const SAMPLE_BUFFER_SIZE: usize = 1024;

const BYTES_PER_SAMPLE: usize = 4 * CHANNEL_COUNT;

// How many bytes should a ring buffer contain
const BUFFER_SIZE: usize = SAMPLE_BUFFER_SIZE * BYTES_PER_SAMPLE;

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
    producers: ArcMut<Vec<Producer<u8>>>,
}

impl AudioStream {
    pub fn new() -> Self {
        let signal = Self::setup();

        Self {
            signal: Arc::new(signal.into()),
            state: Arc::new(StreamState::Idle.into()),
            producers: Arc::new(vec![].into()),
        }
    }

    /// Start processing the stream.
    /// This will push to ring buffers if state is set to Processing.
    pub fn run(&self) {
        println!("Running AudioStream!");

        let state = self.state.clone();
        let producers = self.producers.clone();
        let signal = self.signal.clone();

        // Ensure that processing starts
        *state.lock().unwrap() = StreamState::Processing;

        thread::spawn(move || {
            loop {
                let state = state.lock().unwrap();

                if let StreamState::Idle = *state {
                    // Avoid processing if stream is idle
                    continue;
                }

                let mut producers = producers.lock().expect("Producers not poisoned");
                let mut signal = signal.lock().expect("Signal not poisoned");

                let remaining = producers
                    .iter()
                    .map(|p| p.remaining())
                    .min()
                    .unwrap_or(BUFFER_SIZE);

                if remaining < 2 {
                    let time_to_sleep = Duration::from_millis(20);
                    thread::sleep(time_to_sleep);

                    continue;
                }

                // Calculate how many samples would fit
                let samples_to_render = remaining / BYTES_PER_SAMPLE;

                let samples: Vec<_> = (0..samples_to_render)
                    .into_iter()
                    .flat_map(|_| {
                        let (left, right) = (*signal).get_stereo();
                        [left, right]
                    })
                    .flat_map(|sample| sample.to_ne_bytes())
                    .collect();

                for producer in producers.iter_mut() {
                    producer.push_slice(&samples);
                }
            }
        });
    }

    /// Creates a new AudioStreamSource to read from the stream
    pub fn read(&self) -> AudioStreamConsumer {
        let buffer = RingBuffer::new(BUFFER_SIZE);
        let (producer, consumer) = buffer.split();

        let mut producers = self.producers.lock().unwrap();
        producers.push(producer);

        AudioStreamConsumer::new(consumer)
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

/// Represents a consumer
pub struct AudioStreamConsumer {
    consumer: Consumer<u8>,
}

impl AudioStreamConsumer {
    pub fn new(consumer: Consumer<u8>) -> Self {
        Self { consumer }
    }
}

impl Read for AudioStreamConsumer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let requested_len = buf.len();

        let mut read_len = 0;

        while read_len < requested_len {
            let slice = &mut buf[read_len..];
            let bytes_read = self.consumer.read(slice);

            match bytes_read {
                Ok(len) => read_len += len,
                Err(_) => continue,
            }
        }

        Ok(requested_len)
    }
}
