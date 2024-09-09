use crossbeam::channel::{unbounded, Receiver, Sender};
use parking_lot::Mutex;
use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use super::{Encoder, EncoderIntrospection, Stream};
use crate::{Config, Id, IdType, Introspect, Sample};

pub type ConsumerId = Id<Consumer>;

/// Represents a type that consumes audio data from a [Stream],
/// then provides the encoded data to the end-user.
pub struct Consumer {
    pub id: ConsumerId,
    /// The stream this consumer belongs to.
    stream: Weak<Stream>,
    /// The encoder that will be used to encode the audio data.
    encoder: Arc<Mutex<Box<dyn Encoder>>>,
    /// Receives a unit type when new samples are available
    receiver: Receiver<()>,
}

/// The producer part of a consumer
pub struct Producer {
    encoder: Arc<Mutex<Box<dyn Encoder>>>,
    /// Used to notify the consumer of new samples
    sender: Sender<()>,
}

impl Consumer {
    pub fn new<E>(config: Config, stream: Weak<Stream>) -> (Self, Producer)
    where
        E: Encoder,
    {
        let encoder = E::new(config);
        let boxed_encoder: Box<dyn Encoder> = Box::new(encoder);
        let arced_encoder = Arc::new(Mutex::new(boxed_encoder));

        let (sender, receiver) = unbounded();

        let me = Self {
            stream,
            id: ConsumerId::new(),
            encoder: arced_encoder.clone(),
            receiver,
        };

        let producer = Producer {
            encoder: arced_encoder,
            sender,
        };

        (me, producer)
    }

    /// Returns the content type of the encoded data.
    pub fn content_type(&self) -> String {
        self.encoder.lock().content_type()
    }

    /// Returns the encoded data from the enccoder.
    /// If no data is available yet, it will block until there is.
    pub fn bytes(&self) -> Option<Vec<u8>> {
        loop {
            let mut encoder = self.encoder.lock();
            let bytes = encoder.bytes();

            // Immediately return bytes if they're available
            if let Some(bytes) = bytes {
                return Some(bytes);
            }

            // If we don't drop this before waiting, we will deadlock.
            drop(encoder);

            // Wait for more samples
            let result = self.receiver.recv_timeout(Duration::from_secs(3));

            // If something goes wrong or it times out, just break out of the loop.
            if result.is_err() {
                return None;
            }
        }
    }
}

impl Producer {
    /// Push the provided samples to the consumer and encode them.
    pub fn push(&self, samples: &[Sample]) {
        self.encoder.lock().encode(samples);

        // Notify the consumer of new samples so we can avoid busywaiting
        self.sender.send(()).expect("notifies consumer");
    }
}

#[derive(Debug)]
pub struct ConsumerPairIntrospection {
    pub id: IdType,
    pub encoder: EncoderIntrospection,
}

impl Introspect<ConsumerPairIntrospection> for (&ConsumerId, &Producer) {
    fn introspect(&self) -> ConsumerPairIntrospection {
        ConsumerPairIntrospection {
            id: self.0.value(),
            encoder: self.1.encoder.lock().introspect(),
        }
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
