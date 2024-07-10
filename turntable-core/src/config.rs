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
    /// How many seconds of audio to buffer during playback
    pub buffer_size_in_seconds: f32,
    /// How many seconds of audio to preload into a consumer of a playback stream.
    ///
    /// Lower values increase chance of buffer underruns,
    /// whilst higher values increase latency.
    pub stream_preload_cache_size_in_seconds: f32,
    /// How many seconds of audio can exist between the currently playing offset of a sink.
    ///
    /// Higher values means more memory usage but more lenient seeking, lower values
    /// mean less memory usage but a higher likelihood of buffering when seeking too far from the
    /// playback offset.
    pub sink_preload_window_in_seconds: f32,
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

    /// How many samples are stored in a stream's preload cache
    pub fn stream_preload_cache_size(&self) -> usize {
        (self.stream_preload_cache_size_in_seconds * self.samples_per_sec() as f32) as usize
    }

    /// How many samples between the playback offset can be stored in a sink
    pub fn sink_preload_window_size(&self) -> usize {
        (self.sink_preload_window_in_seconds * self.samples_per_sec() as f32) as usize
    }

    /// Returns the number of samples for any given number of seconds
    pub fn seconds_to_samples(&self, seconds: f32) -> usize {
        (seconds * self.samples_per_sec() as f32) as usize
    }

    /// Returns the number of seconds for any given number of samples
    pub fn samples_to_seconds(&self, samples: usize) -> f32 {
        (samples as f32) / self.samples_per_sec() as f32
    }

    /// Returns the number of samples for any given number of bytes
    pub fn bytes_to_samples(&self, bytes: usize) -> usize {
        bytes / Self::SAMPLES_IN_BYTES
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // The most common sample rate
            sample_rate: 44100,
            // Stereo audio
            channel_count: 2,
            // A small preload size ensures quick loading
            preload_size_in_seconds: 10.0,
            // Keep short songs fully loaded for a nicer seeking experience
            preload_threshold_in_seconds: 60.0 * 3.,
            // 100ms of should be enough to avoid buffer underruns
            buffer_size_in_seconds: 0.1,
            // One second of latency should be OK for most modern networks
            stream_preload_cache_size_in_seconds: 1.,
            // 5 minutes of stored audio is more than enough
            sink_preload_window_in_seconds: 60. * 5.,
        }
    }
}
