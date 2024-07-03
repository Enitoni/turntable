use crossbeam::channel::{unbounded, Receiver, Sender};
use parking_lot::Mutex;
use std::{
    io::Read,
    sync::{Arc, Weak},
    time::Duration,
};

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

    /// Reads the encoded data from the consumer.
    /// Note: This will block if the requested amount is not available yet
    pub fn read(&self, buf: &mut [u8]) -> std::io::Result<usize> {
        let requested_amount = buf.len();
        let mut amount_read = 0;

        loop {
            let mut encoder = self.encoder.lock();

            let slice = &mut buf[amount_read..];
            amount_read += encoder.read(slice)?;

            let remaining = requested_amount.saturating_sub(amount_read);

            if remaining == 0 {
                break;
            }

            // If we don't drop this before waiting, we will deadlock.
            drop(encoder);

            // Wait for more samples
            let result = self.receiver.recv_timeout(Duration::from_secs(3));

            // If something goes wrong or it times out, just break out of the loop.
            if result.is_err() {
                break;
            }
        }

        Ok(amount_read)
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

impl Drop for Consumer {
    fn drop(&mut self) {
        // Remove the consumer from the stream, if it still exists.
        if let Some(s) = self.stream.upgrade() {
            s.remove(self.id)
        }
    }
}
