//! The ingestion is responsible for the process of loading inputs into a sink.

use async_trait::async_trait;
use std::{error::Error, sync::Arc};

use crate::PipelineContext;

mod loading;
mod sink;

pub use loading::*;
pub use sink::*;

/// Represents a type that can ingest sources and load samples into a [Sink].
///
/// This is used for the creation of sinks and loading of external sources.
#[async_trait]
pub trait Ingestion
where
    Self: Sync + Send,
{
    fn new(context: &PipelineContext) -> Self;

    /// Ingests a new source, returning a sink that can be used to play the source.
    async fn ingest<L>(&self, input: L) -> Result<Arc<Sink>, Box<dyn Error>>
    where
        L: IntoLoadable + Send + Sync;

    /// Requests the pipeline to start loading samples into a sink.
    ///
    /// * `sink_id` - The id of the sink to load into.
    /// * `offset` - The offset in samples to start loading from.
    /// * `amount` - The amount of samples to load.
    ///
    /// Note: This function must not be called on the playback thread.
    async fn request_load(&self, sink_id: SinkId, offset: usize, amount: usize);

    /// Clears all inactive sinks from memory.
    fn clear_inactive(&self);
}
