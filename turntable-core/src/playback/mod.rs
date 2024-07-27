use log::info;
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tokio::time::sleep;

mod player;
mod timeline;

pub use player::*;
pub use timeline::*;

use crate::{get_or_create_handle, Ingestion, Output, PipelineContext, SinkManager};

/// The playback type is responsible for managing players, processing playback, and preloading sinks as needed.
pub struct Playback {
    context: PipelineContext,
    output: Arc<Output>,
}

impl Playback {
    pub fn new<I>(
        context: &PipelineContext,
        manager: Arc<SinkManager<I>>,
        output: Arc<Output>,
    ) -> Self
    where
        I: Ingestion + 'static,
    {
        spawn_processing_thread(context);
        spawn_preloading_task(context, manager.clone());
        spawn_cleanup_thread(context, manager);

        Self {
            context: context.clone(),
            output,
        }
    }

    /// Creates a new player, registers it with the output, and returns its id.
    pub fn create_player(&self) -> PlayerContext {
        let player = Player::new(&self.context, self.output.clone());
        let context = player.context();

        info!("Created player #{}", player.id);

        self.output.register_player(player.id);
        self.context.players.insert(player.id, player.into());

        context
    }
}

fn spawn_processing_thread(context: &PipelineContext) {
    let players = context.players.clone();
    let tick_rate = Duration::from_secs_f32(context.config.buffer_size_in_seconds);

    let run = move || {
        let mut next = Instant::now();

        loop {
            for player in players.iter() {
                player.process();
            }

            next += tick_rate;
            spin_sleep::sleep(next - Instant::now())
        }
    };

    thread::spawn(run);
}

fn spawn_cleanup_thread<I>(context: &PipelineContext, manager: Arc<SinkManager<I>>)
where
    I: Ingestion + 'static,
{
    let players = context.players.clone();

    let run = move || loop {
        for player in players.iter() {
            player.clear_superflous();
        }

        let cleared_sinks = manager.clear_inactive();

        if !cleared_sinks.is_empty() {
            info!("Cleared Sinks: {:?}", cleared_sinks)
        }

        thread::sleep(Duration::from_secs(30))
    };

    thread::spawn(run);
}

fn spawn_preloading_task<I>(context: &PipelineContext, manager: Arc<SinkManager<I>>)
where
    I: Ingestion + 'static,
{
    let handle = get_or_create_handle();
    let players = context.players.clone();
    let config = context.config.clone();

    handle.spawn(async move {
        loop {
            for player in players.iter() {
                let preloads = player.preload();

                for preload in preloads {
                    manager
                        .request_load(
                            preload.sink_id,
                            preload.offset,
                            config.preload_size_in_samples(),
                        )
                        .await;
                }
            }

            sleep(Duration::from_secs(1)).await;
        }
    });
}
