use implementors::SymphoniaIngestion;
use std::{error::Error, sync::Arc};
use tokio::runtime::{Handle, Runtime};

mod config;
mod ingestion;
mod output;
mod playback;
mod util;

pub mod implementors;
pub use config::*;
pub use ingestion::*;
pub use output::*;
pub use playback::*;
pub use util::*;

/// The turntable pipeline, facilitating ingestion, playback, and output.
pub struct Pipeline<I> {
    ingestion: Arc<I>,
    playback: Playback,
    output: Arc<Output>,
}

impl<I> Pipeline<I>
where
    I: Ingestion + 'static,
{
    pub fn new(config: Config) -> Pipeline<I> {
        let rt = get_or_create_handle();

        let ingestion = Arc::new(I::new(config.clone()));
        let output = Arc::new(Output::new(config.clone()));
        let playback = Playback::new(rt, config.clone(), ingestion.clone(), output.clone());

        Pipeline {
            output,
            ingestion,
            playback,
        }
    }

    /// Creates a new player and returns its id.
    pub fn create_player(&self) -> PlayerId {
        self.playback.create_player()
    }

    /// Ingests a loader and returns the sink.
    pub async fn ingest<L>(&self, loader: L) -> Result<Arc<Sink>, Box<dyn Error>>
    where
        L: IntoLoadable,
    {
        self.ingestion.ingest(loader).await
    }

    /// Sets the sinks that a player should play.
    pub fn set_sinks(&self, player_id: PlayerId, sinks: Vec<Arc<Sink>>) {
        self.playback.set_sinks(player_id, sinks);
    }

    /// Creates a consumer for a player.
    pub fn consume_player<E>(&self, player_id: PlayerId) -> Arc<Consumer>
    where
        E: Encoder,
    {
        self.output.consume_player::<E>(player_id)
    }
}

impl Default for Pipeline<SymphoniaIngestion> {
    fn default() -> Self {
        Self::new(Config::default())
    }
}
