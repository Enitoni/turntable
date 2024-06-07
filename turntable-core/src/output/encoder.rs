use crate::{Config, Sample};
use std::io::Read;

/// Represents a type that encodes [Sample]s into a desired audio format to be consumed by the end-user.
pub trait Encoder: Read {
    fn new(config: Config) -> Self;

    /// Encodes the provided samples.
    ///
    /// Note: This is potentially a blocking operation.
    fn encode(&mut self, samples: &[Sample]);

    /// Returns the content type of the encoded data.
    fn content_type(&self) -> String;
}
