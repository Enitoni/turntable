use std::{
    fmt::Display,
    ops::Range,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

/// Loads an audio source.
/// This runs on a separate thread to ensure loading does not
/// block playback if samples are still available.
pub struct AudioSourceLoader {
    requests: Arc<Mutex<Vec<LoadRequest>>>,
}

#[derive(Clone)]
pub struct LoadRequest {
    source: Arc<Mutex<AudioSource>>,
    amount: usize,
}

impl AudioSourceLoader {
    /// Loading typically doesn't happen often,
    /// so we can save a lot of CPU usage here.
    const SLEEP_DURATION: Duration = Duration::from_millis(500);

    pub fn new() -> Self {
        Self {
            requests: Default::default(),
        }
    }

    /// Request samples to be loaded into a source
    pub fn request(&self, source: Arc<Mutex<AudioSource>>, amount: usize) {
        let mut requests = self.requests.lock().unwrap();

        requests.push(LoadRequest { source, amount });
    }

    /// Start the thread to load audio
    pub fn run(&self) {
        thread::spawn({
            let requests = self.requests.clone();

            move || loop {
                let mut requests = requests.lock().unwrap();

                while let Some(request) = requests.pop() {
                    let mut source = request.source.lock().unwrap();
                    let result = source.load(request.amount);

                    match result {
                        Ok(_size) => {
                            // TODO: Deal with this someday.
                        }
                        Err(err) => {
                            source.dead = err.is_fatal();

                            // Try again next iteration.
                            if !source.dead {
                                requests.push(request.clone());
                                continue;
                            }
                        }
                    };
                }

                thread::sleep(Self::SLEEP_DURATION);
            }
        });
    }
}

/// A normalized audio source.
///
/// This should have zero samples stripped from beginning and end to
/// ensure optimal gapless.
///
/// This is also always 32-bit floating point audio,
/// though this may change in the future if needed.
pub struct AudioSource {
    id: usize,
    /// Currently allocated samples.
    pub samples: Vec<f32>,
    /// Total amount of samples.
    pub length: usize,
    /// Amount of samples consumed so far
    offset: usize,
    /// Source of the samples.
    source: Box<dyn SampleSource + Sync + Send>,
    /// This source is dead and should be skipped.
    dead: bool,
}

impl AudioSource {
    /// Amount of samples to keep in memory.
    /// This is equivalent to roughly one minute of 44.1khz sample rate.
    const BUFFER_SIZE: usize = 1024 * 1024 * 24;

    /// Tries to load the requested amount of samples.
    /// Returns the amount of samples loaded.
    fn load(&mut self, amount: usize) -> Result<usize, SampleSourceError> {
        self.deallocate_used();

        let remaining = self.length - self.offset;
        let amount_to_fetch = amount.min(remaining);

        let buffer = vec![0.; amount_to_fetch];
        let samples_read = self.source.read(&buffer)?;

        self.samples.extend(buffer);
        self.offset += samples_read;

        Ok(samples_read)
    }

    /// Clear out samples that have been consumed if the
    /// allocated amount of samples exceeds a certain amount.
    fn deallocate_used(&mut self) {
        let amount_allocated = self.samples.len();

        let limit = Self::BUFFER_SIZE as i32;
        let overflow = (amount_allocated as i32) - limit;

        // Remove a third of the buffer size
        if overflow > limit / 3 {
            let amount_to_remove = 0..(limit / 3) as usize;
            self.samples.drain(amount_to_remove);
        }
    }

    pub fn samples(&self, range: Range<usize>) -> &[f32] {
        // This is never below 0.
        let absolute_start = self.offset - self.samples.len();

        let relative_start = range.start - absolute_start;
        let relative_end = range.end.min(self.offset);

        &self.samples[relative_start..relative_end]
    }

    pub fn is_finished(&self) -> bool {
        self.offset == self.length || self.dead
    }
}

#[derive(Debug)]
pub enum SampleSourceError {
    /// Loading samples failed for whatever reason.
    /// This could be network related, for example.
    LoadFailed { reason: String },
    /// The SampleSource cannot be used anymore,
    /// and should be skipped.
    Fatal { reason: String },
}

impl SampleSourceError {
    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::Fatal { .. })
    }
}

impl Display for SampleSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SampleSourceError::LoadFailed { reason } => {
                write!(f, "Source failed to load samples: \"{}\"", reason)?;
            }
            SampleSourceError::Fatal { reason } => write!(
                f,
                "Source is skipped because of fatal error: \"{}\"",
                reason
            )?,
        };

        Ok(())
    }
}

/// A source of audio samples
pub trait SampleSource {
    /// How many samples does this source return
    fn length(&self) -> usize;

    fn read(&mut self, buf: &[f32]) -> Result<usize, SampleSourceError>;
}
