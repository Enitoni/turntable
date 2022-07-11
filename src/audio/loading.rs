use std::{
    fmt::Display,
    io::Read,
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
                // Sleeping first ensures mutexes aren't locked
                thread::sleep(Self::SLEEP_DURATION);

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

                            println!("{}", err);

                            // Try again next iteration.
                            if !source.dead {
                                requests.push(request.clone());
                                continue;
                            }
                        }
                    };
                }
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
    //id: usize,
    /// Currently allocated samples.
    pub samples: Vec<f32>,
    /// Total amount of samples.
    pub length: usize,
    /// Amount of samples loaded so far
    offset: usize,
    /// Consumed
    consumed: usize,
    /// Source of the samples.
    source: Box<dyn SampleSource + Sync + Send>,
    /// This source is dead and should be skipped.
    dead: bool,
}

impl AudioSource {
    /// Amount of samples to keep in memory.
    /// This is equivalent to roughly one minute of 44.1khz sample rate.
    const BUFFER_SIZE: usize = 1024 * 1024 * 24;

    pub fn new<T: 'static + SampleSource + Sync + Send>(source: T) -> Self {
        let len = source.length();

        Self {
            samples: Default::default(),
            source: Box::new(source),
            length: len,
            dead: false,
            consumed: 0,
            offset: 0,
        }
    }

    /// Tries to load the requested amount of samples.
    /// Returns the amount of samples loaded.
    fn load(&mut self, amount: usize) -> Result<usize, SampleSourceError> {
        self.deallocate_used();

        let remaining = self.length - self.offset;
        let amount_to_fetch = amount.min(remaining);

        let mut buffer = vec![0.; amount_to_fetch];
        let samples_read = self.source.read(&mut buffer)?;

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

    pub fn samples(&mut self, range: Range<usize>) -> &[f32] {
        // This is never below 0.
        let absolute_start = self.offset - self.samples.len();
        let safe_end = self.samples.len();

        // If the buffer has been deallocated, this will ensure index stays within vec
        let relative_start = range.start - absolute_start;
        let relative_end = relative_start + range.len();

        let start = relative_start.min(safe_end);
        let end = relative_end.min(safe_end);

        let samples = &self.samples[start..end];
        self.consumed += samples.len();

        dbg!(
            &range,
            self.offset,
            self.consumed,
            self.length,
            self.samples.len(),
            self.is_finished()
        );

        samples
    }

    pub fn is_finished(&self) -> bool {
        dbg!("UH", self.consumed, self.length, self.dead);
        self.consumed == self.length || self.dead
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

    fn read(&mut self, buf: &mut [f32]) -> Result<usize, SampleSourceError>;
}

impl SampleSource for std::fs::File {
    fn length(&self) -> usize {
        let meta = self.metadata().expect("Read metadata");

        (meta.len() as usize) / 4
    }

    fn read(&mut self, buf: &mut [f32]) -> Result<usize, SampleSourceError> {
        let mut intermediate = vec![0; buf.len() * 4];

        <Self as Read>::read(self, &mut intermediate).map_err(|e| {
            SampleSourceError::LoadFailed {
                reason: e.to_string(),
            }
        })?;

        let samples: Vec<_> = intermediate
            .chunks_exact(4)
            .map(|byte| f32::from_le_bytes([byte[0], byte[1], byte[2], byte[3]]))
            .collect();

        buf[..samples.len()].copy_from_slice(&samples);
        Ok(samples.len())
    }
}
