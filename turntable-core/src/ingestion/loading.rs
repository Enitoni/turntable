use async_trait::async_trait;
use std::{error::Error, sync::Arc};

use crate::Config;

use super::sink::Sink;

/// Represents a type that can load raw bytes from any source.
/// Activated inputs typically implement this trait.
///
/// This is different from a _Loader_ which is responsible for loading data from a [Loadable] into a [Sink].
///
/// Note: Although an offset can be provided, there is no guarantee that the correct data will be loaded.
/// Likewise, the amount of data may be less than the requested amount.
///
/// However, this depends on the implementation of the loadable.
#[async_trait]
pub trait Loadable
where
    Self: 'static + Sync + Send,
{
    /// Attempts to load raw bytes from the source.
    ///
    /// * `offset` - The offset to start loading from.
    /// * `amount` - The amount of bytes to load.
    async fn load(&self, offset: usize, amount: usize) -> Result<LoadResult, Box<dyn Error>>;

    /// Attempts to probe the source for metadata.
    /// For now, this is only used to determine the length of the source.
    async fn probe(&self) -> Result<ProbeResult, Box<dyn Error>>;

    /// Shorthand for creating a [BoxedLoadable].
    fn boxed(self) -> BoxedLoadable
    where
        Self: Sized,
    {
        BoxedLoadable(Box::new(self))
    }
}

/// The result of a load operation triggered by a [Loadable].
///
/// Note: This is not clonable, because the data could be too inefficient to clone.
#[derive(Debug)]
pub struct LoadResult {
    /// The offset in bytes where the data was loaded from.
    ///
    /// Note: This may be slightly different from the offset provided.
    pub at_offset: usize,
    /// The data that was loaded.
    pub bytes: Vec<u8>,
    /// Whether this is the last chunk of data.
    pub end_reached: bool,
}

/// The result of a probe operation triggered by a [Loadable].
/// This is used for calculating offsets and length during loading.
#[derive(Debug, Clone)]
pub enum ProbeResult {
    /// The source is raw audio
    Raw {
        // The length of the source in bytes
        length: usize,
        // The sample rate of the source in Hz
        sample_rate: usize,
    },
    /// The source is compressed audio
    Compressed {
        // The size of the compressed audio frame in bytes
        // If this is None, bit_rate is used with length instead.
        frame_size: Option<usize>,
        // The bitrate of the audio
        bit_rate: usize,
        // The length of the source in bytes
        length: usize,
        // The sample rate of the source in Hz
        sample_rate: usize,
    },
    /// The source is a live stream, or something else that is not seekable
    Unseekable,
}

impl ProbeResult {
    /// Returns the amount of samples, if applicable.
    pub fn length_in_samples(&self, config: &Config) -> Option<usize> {
        match self {
            ProbeResult::Raw { length, .. } => Some(config.bytes_to_samples(*length)),
            ProbeResult::Compressed {
                length, bit_rate, ..
            } => Some(length / (bit_rate / 8)),
            ProbeResult::Unseekable => None,
        }
    }

    // Returns the byte offset from a desired sample offset
    pub fn byte_offset(&self, sample_offset: usize) -> usize {
        match self {
            ProbeResult::Raw { length, .. } => sample_offset,
            ProbeResult::Compressed {
                length,
                bit_rate,
                sample_rate,
                ..
            } => (sample_offset * bit_rate) / (8 * sample_rate),
            ProbeResult::Unseekable => 0,
        }
    }
}

/// [Loadable] trait object.
pub struct BoxedLoadable(Box<dyn Loadable>);

#[async_trait]
impl Loadable for BoxedLoadable {
    async fn load(&self, offset: usize, amount: usize) -> Result<LoadResult, Box<dyn Error>> {
        self.0.load(offset, amount).await
    }

    async fn probe(&self) -> Result<ProbeResult, Box<dyn Error>> {
        self.0.probe().await
    }
}

/// Represents a type that loads samples from a [Loadable] into a [Sink].
///
/// Usually, this is just an implementation that uses ffmpeg, but it could be any other type of loader.
#[async_trait]
pub trait Loader {
    /// Instantiates the loader.
    /// Implementors are expected to store the [Loadable], [ProbeResult], and [Sink] in the type.
    fn new<L: Loadable>(
        config: Config,
        probe_result: ProbeResult,
        loadable: L,
        sink: Arc<Sink>,
    ) -> Self;

    /// Loads samples from the [Loadable] into the [Sink].
    ///
    /// * `offset` - The offset in samples to start loading from.
    /// * `amount` - The amount of samples to load.
    ///
    /// The implementor is expected to do the following:
    /// 1. When this is called, the sink's state is set to `Loading`
    /// 2. On a successful load, the samples are written to the [Sink].
    /// 3. When the end is reached, the sink is sealed.
    /// 4. If there is an error, the sink's state is set to `Error` with the relevant error message.
    async fn load(&self, offset: usize, amount: usize);
}
