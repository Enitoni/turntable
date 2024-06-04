//! The ingestion is responsible for the process of loading inputs into a sink.

use dashmap::DashMap;
use std::{error::Error, sync::Arc};

use crate::Config;

mod loading;
mod sink;

pub use loading::*;
pub use sink::*;

/// The ingestion is responsible for all the processes of loading sources, so that they can be played.
pub struct Ingestion<L> {
    config: Config,
    /// The currently active loaders.
    loaders: DashMap<SinkId, Arc<L>>,
    /// The currently active sinks.
    sinks: DashMap<SinkId, Arc<Sink>>,
}

impl<L> Ingestion<L>
where
    L: Loader + Send + Sync + 'static,
{
    pub fn new(config: Config) -> Self {
        Self {
            config,
            loaders: Default::default(),
            sinks: Default::default(),
        }
    }

    /// Ingests a new source, returning a sink that can be used to play the source.
    /// If something goes wrong during probing, the result will be an error.
    pub async fn ingest<LB>(&self, loadable: LB) -> Result<Arc<Sink>, Box<dyn Error>>
    where
        LB: Loadable,
    {
        let probe_result = loadable.probe().await?;
        let length_in_samples = probe_result
            .length
            .map(|sec| self.config.seconds_to_samples(sec));

        let sink: Arc<_> = Sink::new(length_in_samples).into();
        let loader = L::new(
            self.config.clone(),
            probe_result.clone(),
            loadable,
            sink.clone(),
        );

        self.loaders.insert(sink.id, loader.into());
        self.sinks.insert(sink.id, sink.clone());

        Ok(sink)
    }

    /// Requests for a loader to start loading samples into a sink.
    pub fn request_load(&self, sink_id: SinkId, offset: usize, amount: usize) {
        let loader = self
            .loaders
            .get(&sink_id)
            .expect("request_load() is not called for a sink that does not exist")
            .value()
            .clone();

        tokio::spawn(async move { loader.load(offset, amount).await });
    }
}
