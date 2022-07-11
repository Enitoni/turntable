use fundsp::hacker32::*;

use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use super::{AudioBufferConsumer, BufferRegistry, Player};

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
    player: ArcMut<Player>,
    state: ArcMut<StreamState>,
    registry: Arc<BufferRegistry>,
}

impl AudioStream {
    pub fn new(player: ArcMut<Player>) -> Self {
        Self {
            state: Arc::new(StreamState::Idle.into()),
            registry: BufferRegistry::new().into(),
            player,
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
            let player = Arc::clone(&self.player);

            move || {
                // Calculate optimal laziness
                let total_samples = (SAMPLE_BUFFER_SIZE * CHANNEL_COUNT) as f32;
                let seconds_per_sample = 1. / SAMPLE_RATE as f32;

                let optimal_sleep_time = total_samples * seconds_per_sample / WAKE_UP_DIVISOR;
                let time_to_sleep = Duration::from_secs_f32(optimal_sleep_time);

                let _now = Instant::now();

                loop {
                    thread::sleep(time_to_sleep);
                    let state = state.lock().unwrap();

                    if let StreamState::Idle = *state {
                        // Avoid processing if stream is idle
                        continue;
                    }

                    // Ensure dead buffers are removed
                    registry.recycle();

                    let mut player = player.lock().unwrap();
                    let remaining = registry.samples_remaining();

                    // This will deadlock if dead buffers are not removed
                    if remaining < 2 {
                        continue;
                    }

                    let mut samples = vec![0.; remaining];
                    player.next_chunk(&mut samples);

                    let samples_as_bytes: Vec<_> = samples
                        .into_iter()
                        .flat_map(|sample| sample.to_le_bytes())
                        .collect();

                    // Push the samples into the ring buffers
                    registry.write_byte_samples(&samples_as_bytes);
                }
            }
        });
    }

    /// Creates a new AudioStreamSource to read from the stream
    pub fn get_consumer(&self) -> AudioBufferConsumer {
        self.registry.get_consumer()
    }
}
