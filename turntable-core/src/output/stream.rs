use std::sync::{Arc, Weak};

use crate::{Config, Sample};

/// A stream is the destination of a [Player], and manages consumers for said player.
///
/// Consumers provide encoded audio data to the end-user.
pub struct Stream {
    config: Config,
    /// A weak reference is required because dropped consumers need to be removed.
    me: Weak<Stream>,
    /// A preloaded cache of samples used to instantly fill a consumer,
    /// so that there isn't a delay before a consumer returns data.
    preload_cache: Vec<Sample>,
}

impl Stream {
    pub fn new(config: Config) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            config,
            me: me.clone(),
            preload_cache: Default::default(),
        })
    }
}
