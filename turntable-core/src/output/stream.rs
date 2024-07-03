use std::sync::{Arc, Weak};

use dashmap::DashMap;
use parking_lot::Mutex;

use super::{Consumer, ConsumerId, Encoder};
use crate::{Config, Producer, Sample};

/// A stream is the destination of a [Player], and manages consumers for said player.
///
/// Consumers provide encoded audio data to the end-user.
pub struct Stream {
    config: Config,
    /// A weak reference is required because dropped consumers need to be removed.
    me: Weak<Stream>,
    /// A preloaded cache of samples used to instantly fill a consumer,
    /// so that there isn't a delay before a consumer returns data.
    preload_cache: Mutex<Vec<Sample>>,
    /// The producer parts of consumers that have been created for this stream.
    producers: DashMap<ConsumerId, Producer>,
}

impl Stream {
    pub fn new(config: Config) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            config,
            me: me.clone(),
            producers: Default::default(),
            preload_cache: Default::default(),
        })
    }

    /// Gets a new consumer for this stream.
    pub fn consume<E>(&self) -> Consumer
    where
        E: Encoder,
    {
        let consumer = Consumer::new::<E>(self.config.clone(), self.me.clone());
        let preload_cache = self.preload_cache.lock();

        let producer = consumer.producer();
        producer.push(&preload_cache);

        self.producers.insert(consumer.id, producer);

        consumer
    }

    /// Removes a producer from this stream.
    pub fn remove(&self, consumer_id: ConsumerId) {
        self.producers.remove(&consumer_id);
    }

    /// Push new samples to the stream.
    ///
    /// Note: This function must not be called on the playback thread.
    pub fn push(&self, samples: &[Sample]) {
        for producer in self.producers.iter() {
            producer.push(samples);
        }

        self.push_preload(samples)
    }

    /// Pushes samples to the preload cache.
    pub fn push_preload(&self, samples: &[Sample]) {
        let mut preload_cache = self.preload_cache.lock();

        preload_cache.extend_from_slice(samples);

        let preload_size = self.config.stream_preload_cache_size();
        let amount_overflowing = preload_cache.len().saturating_sub(preload_size);

        if amount_overflowing > 0 {
            preload_cache.drain(..amount_overflowing);
        }
    }
}
