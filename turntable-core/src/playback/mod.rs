use std::sync::Arc;

mod timeline;

use crate::Ingestion;

/// The playback type is responsible for managing players, processing playback, and preloading sinks as needed.
pub struct Playback<I> {
    ingestion: Arc<I>,
}

impl<I> Playback<I>
where
    I: Ingestion,
{
    pub fn new(ingestion: Arc<I>) -> Self {
        Self { ingestion }
    }
}