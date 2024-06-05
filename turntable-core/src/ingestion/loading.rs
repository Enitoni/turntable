use async_trait::async_trait;
use std::{error::Error, io::SeekFrom};

/// Represents a type that can load raw audio bytes from any source.
/// Activated inputs typically implement this trait.
#[async_trait]
pub trait Loadable
where
    Self: 'static + Sync + Send,
{
    /// Attempts to load raw bytes from the source.
    /// If a seek was made, this reads from the seeked position.
    ///
    /// * `buf` - The buffer to read into.
    async fn read(&self, buf: &mut [u8]) -> Result<ReadResult, Box<dyn Error>>;

    /// Returns the length of the source, if known.
    /// If this is [None], it's assumed that the source is live.
    async fn length(&self) -> Option<LoaderLength>;

    /// Returns whether the source is seekable.
    /// An assumption is made by default that the source is seekable if it has a length.
    ///
    /// Override this if you need to make a different assumption.
    async fn seekable(&self) -> bool {
        self.length().await.is_some()
    }

    /// Attempts to seek to a given position.
    ///
    /// * `seek` - The position to seek to.
    async fn seek(&self, seek: SeekFrom) -> Result<usize, Box<dyn Error>>;

    /// Shorthand for creating a [BoxedLoadable].
    fn boxed(self) -> BoxedLoadable
    where
        Self: Sized,
    {
        BoxedLoadable(Box::new(self))
    }
}

/// The medium of length that a loader is aware of.
#[derive(Debug, Clone, Copy)]
pub enum LoaderLength {
    /// The length is known in seconds.
    Time(f32),
    /// The length is known in bytes.
    Bytes(usize),
}

/// The result of a read operation triggered by a [Loadable].
#[derive(Debug, Clone, Copy)]
pub enum ReadResult {
    // There is more data to read. Number is the number of bytes read.
    More(usize),
    // End of stream reached. Number is the number of bytes read.
    End(usize),
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

/// [Loadable] trait object.
pub struct BoxedLoadable(Box<dyn Loadable>);

#[async_trait]
impl Loadable for BoxedLoadable {
    async fn read(&self, buf: &mut [u8]) -> Result<ReadResult, Box<dyn Error>> {
        self.0.read(buf).await
    }

    async fn length(&self) -> Option<LoaderLength> {
        self.0.length().await
    }

    async fn seekable(&self) -> bool {
        self.0.seekable().await
    }

    async fn seek(&self, seek: SeekFrom) -> Result<usize, Box<dyn Error>> {
        self.0.seek(seek).await
    }
}
