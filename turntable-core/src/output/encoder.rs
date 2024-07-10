use crate::{Config, Sample};

/// Represents a type that encodes [Sample]s into a desired audio format to be consumed by the end-user.
pub trait Encoder
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

    /// Consumes the bytes currently encoded in the encoder.
    fn bytes(&mut self) -> Option<Vec<u8>>;

    /// Returns the content type of the encoded data.
    fn content_type(&self) -> String;

    /// Returns a human friendly name
    fn name() -> String
    where
        Self: Sized;
}
