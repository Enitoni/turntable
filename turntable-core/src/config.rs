use std::mem::size_of;

/// A single audio sample
pub type Sample = f32;

/// The configuration of the audio pipeline
#[derive(Debug, Clone)]
pub struct Config {
    /// The rate of samples per second
    pub sample_rate: usize,
    /// The number of channels in the audio stream
    pub channel_count: usize,
    /// How many seconds of audio to preload
    pub preload_size_in_seconds: f32,
    /// How many seconds can be left before more is preloaded
    pub preload_threshold_in_seconds: f32,
    /// How many milliseconds of audio to buffer during playback
    pub buffer_size_in_seconds: f32,
    /// How much delay before playback starts, lower values increase chances of buffer underruns
    pub latency_in_seconds: f32,
}

impl Config {
    pub const SAMPLES_IN_BYTES: usize = size_of::<Sample>();

    /// How many samples exist in a second
    pub fn samples_per_sec(&self) -> usize {
        self.sample_rate * self.channel_count
    }

    /// How many samples are preloaded
    pub fn preload_size_in_samples(&self) -> usize {
        (self.preload_size_in_seconds * self.samples_per_sec() as f32) as usize
    }

    /// How many samples can be left before more is preloaded
    pub fn preload_threshold_in_samples(&self) -> usize {
        (self.preload_threshold_in_seconds * self.samples_per_sec() as f32) as usize
    }

    /// How many samples are buffered during playback
    pub fn buffer_size_in_samples(&self) -> usize {
        (self.buffer_size_in_seconds * self.samples_per_sec() as f32) as usize
    }

    /// How often are samples processed in seconds
    pub fn playback_tick_rate(&self) -> f32 {
        self.buffer_size_in_seconds
    }

    /// How many samples to pad the start of playback with
    pub fn latency_in_samples(&self) -> usize {
        (self.latency_in_seconds * self.samples_per_sec() as f32) as usize
    }
}