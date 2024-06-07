use crate::{Config, Sample};
use std::io::Read;

/// Represents a type that encodes [Sample]s into a desired audio format to be consumed by the end-user.
pub trait Encoder: Read
where
    Self: 'static + Send + Sync,
{
    fn new(config: Config) -> Self
    where
        Self: Sized;

    /// Encodes the provided samples.
    ///
    /// Note: This is potentially a blocking operation.
    fn encode(&mut self, samples: &[Sample]);

    /// Returns the content type of the encoded data.
    fn content_type(&self) -> String;
}
