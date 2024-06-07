use std::sync::{Arc, Weak};

use dashmap::DashMap;
use parking_lot::Mutex;

use crate::{Config, Id, Sample};

use super::{Consumer, ConsumerId, Encoder};

pub type StreamId = Id<Stream>;

/// A stream is the destination of a [Player], and manages consumers for said player.
///
/// Consumers provide encoded audio data to the end-user.
pub struct Stream {
    id: StreamId,
    config: Config,
    /// A weak reference is required because dropped consumers need to be removed.
    me: Weak<Stream>,
    /// A preloaded cache of samples used to instantly fill a consumer,
    /// so that there isn't a delay before a consumer returns data.
    preload_cache: Mutex<Vec<Sample>>,
    /// The consumers that have been created for this stream.
    consumers: DashMap<ConsumerId, Arc<Consumer>>,
}

impl Stream {
    pub fn new(config: Config) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            config,
            me: me.clone(),
            id: StreamId::new(),
            consumers: Default::default(),
            preload_cache: Default::default(),
        })
    }

    /// Gets a new consumer for this stream.
    pub fn consume<E>(&self) -> Arc<Consumer>
    where
        E: Encoder,
    {
        let consumer: Arc<_> = Consumer::new::<E>(self.config.clone(), self.me.clone()).into();
        let preload_cache = self.preload_cache.lock();

        self.consumers.insert(consumer.id, consumer.clone());

        consumer.push(&preload_cache);
        consumer
    }

    /// Removes a consumer from this stream.
    pub fn remove(&self, consumer_id: ConsumerId) {
        self.consumers.remove(&consumer_id);
    }

    /// Push new samples to the stream.
    ///
    /// Note: This function must not be called on the playback thread.
    pub fn push(&self, samples: &[Sample]) {
        let consumers: Vec<_> = self.consumers.iter().map(|c| c.clone()).collect();

        for consumer in consumers {
            consumer.push(samples);
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
