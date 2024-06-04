use async_trait::async_trait;
use std::error::Error;

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
#[derive(Debug)]
pub struct ProbeResult {
    /// The length of the source in seconds.
    ///
    /// If this is `None`, the length is unknown.
    /// For example, the source may be a live stream.
    pub length: Option<f32>,
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

impl From<Box<dyn Loadable>> for BoxedLoadable {
    fn from(loadable: Box<dyn Loadable>) -> Self {
        Self(loadable)
    }
}
