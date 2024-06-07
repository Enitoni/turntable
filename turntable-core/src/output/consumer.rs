use parking_lot::Mutex;
use std::{io::Read, sync::Weak};

use super::{Encoder, Stream};
use crate::{Config, Id, Sample};

pub type ConsumerId = Id<Consumer>;

/// Represents a type that consumes audio data from a [Stream],
/// then provides the encoded data to the end-user.
pub struct Consumer {
    pub id: ConsumerId,
    /// The stream this consumer belongs to.
    stream: Weak<Stream>,
    /// The encoder that will be used to encode the audio data.
    encoder: Mutex<Box<dyn Encoder>>,
}

impl Consumer {
    pub fn new<E>(config: Config, stream: Weak<Stream>) -> Self
    where
        E: Encoder,
    {
        let encoder = E::new(config);

        Self {
            stream,
            id: ConsumerId::new(),
            encoder: Mutex::new(Box::new(encoder)),
        }
    }

    /// Push the provided samples to the consumer and encode them.
    pub fn push(&self, samples: &[Sample]) {
        let mut encoder = self.encoder.lock();
        encoder.encode(samples);
    }

    /// Returns the content type of the encoded data.
    pub fn content_type(&self) -> String {
        self.encoder.lock().content_type()
    }

    /// Reads the encoded data from the consumer.
    pub fn read(&self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut encoder = self.encoder.lock();
        encoder.read(buf)
    }
}

impl Drop for Consumer {
    fn drop(&mut self) {
        // Remove the consumer from the stream, if it still exists.
        if let Some(s) = self.stream.upgrade() {
            s.remove(self.id)
        }
    }
}
