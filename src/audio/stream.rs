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

pub const BYTES_PER_SAMPLE: usize = 4 * CHANNEL_COUNT;

// How many bytes should a ring buffer contain
pub const BUFFER_SIZE: usize = (SAMPLE_RATE * BYTES_PER_SAMPLE) * 2;

// How many time to wake up the thread during a sleep
const WAKE_UP_DIVISOR: f32 = 5.;

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
    const BUFFER_DURATION: Duration = Duration::from_millis(100);

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
                let samples_per_sec = SAMPLE_RATE * CHANNEL_COUNT;
                let samples_to_render = {
                    let sps = samples_per_sec as u128;
                    let duration = Self::BUFFER_DURATION.as_millis();

                    (sps * duration / 1000) as usize
                };

                loop {
                    let now = Instant::now();
                    {
                        let state = state.lock().unwrap();

                        if let StreamState::Idle = *state {
                            // Avoid processing if stream is idle
                            continue;
                        }
                    }

                    // Ensure dead buffers are removed
                    registry.recycle();

                    let mut player = player.lock().unwrap();
                    let mut samples = vec![0.; samples_to_render];

                    player.read(&mut samples);

                    let samples_as_bytes: Vec<_> = samples
                        .into_iter()
                        .flat_map(|sample| sample.to_le_bytes())
                        .collect();

                    // Push the samples into the ring buffers
                    registry.write_byte_samples(&samples_as_bytes);

                    let elapsed = now.elapsed();
                    let elapsed_micros = elapsed.as_micros();

                    let duration_micros = Self::BUFFER_DURATION.as_micros();
                    let corrected = duration_micros
                        .checked_sub(elapsed_micros)
                        .unwrap_or_default();

                    spin_sleep::sleep(Duration::from_micros(corrected as u64));
                }
            }
        });
    }

    /// Creates a new AudioStreamSource to read from the stream
    pub fn get_consumer(&self) -> AudioBufferConsumer {
        self.registry.get_consumer()
    }
}
