//! The ingestion is responsible for the process of loading inputs into a sink.

use async_trait::async_trait;
use dashmap::DashMap;
use std::{error::Error, sync::Arc};

use crate::PipelineContext;

mod loading;
mod sink;

pub use loading::*;
pub use sink::*;

/// Represents a type that can ingest sources and load samples into a [Sink].
#[async_trait]
pub trait Ingestion
where
    Self: Sync + Send + 'static,
{
    /// A loader is an implementation specific detail of an ingestion, which is associated with a sink id.
    type Loader: Sync + Send;

    fn new(context: &PipelineContext) -> Self;

    /// Ingests a new source, returning an [Ingest] when successful.
    async fn ingest<L>(&self, input: L) -> Result<Ingest<Self::Loader>, Box<dyn Error>>
    where
        L: IntoLoadable + Send + Sync;

    /// Requests the pipeline to start loading samples into a sink.
    /// Note: This function must not be called on the playback thread.
    async fn request_load(&self, request: LoadRequest<Self::Loader>);

    fn name() -> String;
}

/// Represents the outcome of ingesting a loadable
pub struct Ingest<L> {
    /// The length that the ingestion has calculated the audio is
    pub expected_length: Option<usize>,
    pub loader: L,
}

/// Represents a request to load into a sink
pub struct LoadRequest<L> {
    /// The sink write guard to write the samples with
    pub write_guard: WriteGuard,
    /// The loader associated with the sink
    pub loader: Arc<L>,
    /// The offset in samples to load from
    pub offset: usize,
    /// The preferred amount of samples to load
    pub amount: usize,
}

/// A manager struct that manages the creation, activation, loading, and clearing of sinks
pub struct SinkManager<I>
where
    I: Ingestion,
{
    context: PipelineContext,
    loaders: DashMap<SinkId, Arc<I::Loader>>,
    ingestion: I,
}

impl<I> SinkManager<I>
where
    I: Ingestion,
{
    pub fn new(context: &PipelineContext, ingestion: I) -> Self {
        Self {
            loaders: Default::default(),
            context: context.clone(),
            ingestion,
        }
    }

    /// Prepares an inactive sink
    pub fn prepare(&self) -> Arc<Sink> {
        let sink = Arc::new(Sink::prepare(&self.context));

        self.context.sinks.insert(sink.id, sink.clone());
        sink
    }

    /// Attempts to activate a sink with a loader
    pub async fn activate<L>(&self, sink_id: SinkId, loader: L)
    where
        L: IntoLoadable + Send + Sync,
    {
        let sink = self
            .context
            .sinks
            .get(&sink_id)
            .expect("sink exists when trying to activate");

        let guard = sink.activate();
        let ingest = self.ingestion.ingest(loader).await;

        match ingest {
            Ok(ingest) => {
                self.loaders.insert(sink_id, ingest.loader.into());
                guard.activate(ingest.expected_length);
            }
            Err(err) => guard.fail(&err.to_string()),
        };
    }

    /// Requests the ingestion to load data into a sink
    pub async fn request_load(&self, sink_id: SinkId, offset: usize, amount: usize) {
        let sink = self
            .context
            .sinks
            .get(&sink_id)
            .expect("sink exists when trying to load");

        let loader = self
            .loaders
            .get(&sink_id)
            .expect("loader exists when trying to load")
            .clone();

        self.ingestion
            .request_load(LoadRequest {
                write_guard: sink.write(),
                loader,
                offset,
                amount,
            })
            .await
    }

    pub fn clear_inactive(&self) -> Vec<SinkId> {
        let clearable_sink_ids: Vec<_> = self
            .context
            .sinks
            .iter()
            .filter_map(|s| if s.is_clearable() { Some(s.id) } else { None })
            .collect();

        self.loaders
            .retain(|id, _| !clearable_sink_ids.contains(id));

        self.context
            .sinks
            .retain(|id, _| !clearable_sink_ids.contains(id));

        clearable_sink_ids
    }
}
