use dashmap::DashMap;
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

use crate::{get_or_create_handle, Config, Ingestion, Output, PipelineContext, Sink};

/// The playback type is responsible for managing players, processing playback, and preloading sinks as needed.
pub struct Playback {
    context: PipelineContext,
    output: Arc<Output>,
    /// All the players that exist in the playback.
    players: Arc<DashMap<PlayerId, Player>>,
}

impl Playback {
    pub fn new<I>(context: &PipelineContext, ingestion: Arc<I>, output: Arc<Output>) -> Self
    where
        I: Ingestion + 'static,
    {
        let config = context.config.clone();
        let players = Arc::new(DashMap::new());

        spawn_processing_thread(config.clone(), players.clone());
        spawn_preloading_task(config.clone(), ingestion, players.clone());

        Self {
            context: context.clone(),
            players,
            output,
        }
    }

    pub fn set_sinks(&self, player_id: PlayerId, sinks: Vec<Arc<Sink>>) {
        if let Some(player) = self.players.get(&player_id) {
            player.set_sinks(sinks);
        }
    }

    /// Creates a new player, registers it with the output, and returns its id.
    pub fn create_player(&self) -> PlayerId {
        let player = Player::new(&self.context, self.output.clone());
        let player_id = player.id;

        self.output.register_player(player.id);
        self.players.insert(player.id, player);

        player_id
    }
}

fn spawn_processing_thread(config: Config, players: Arc<DashMap<PlayerId, Player>>) {
    let run = move || loop {
        let now = Instant::now();

        for player in players.iter() {
            player.process();
        }

        wait_for_next(now, config.clone());
    };

    thread::spawn(run);
}

fn spawn_preloading_task<I>(
    config: Config,
    ingestion: Arc<I>,
    players: Arc<DashMap<PlayerId, Player>>,
) where
    I: Ingestion + 'static,
{
    let handle = get_or_create_handle();

    handle.spawn(async move {
        loop {
            for player in players.iter() {
                let preloads = player.preload();

                for preload in preloads {
                    ingestion
                        .request_load(
                            preload.sink_id,
                            preload.offset,
                            config.preload_size_in_samples(),
                        )
                        .await;

                    player.clear_superflous();
                }

                ingestion.clear_inactive();
            }

            sleep(Duration::from_secs(5)).await;
        }
    });
}

fn wait_for_next(now: Instant, config: Config) {
    let elapsed = now.elapsed();
    let elapsed_micros = elapsed.as_micros();
    let elapsed_millis = elapsed_micros / 1000;

    let duration = Duration::from_secs_f32(config.buffer_size_in_seconds);
    let duration_micros = duration.as_micros();

    if elapsed_millis > config.samples_per_sec() as u128 / 10000 {
        // Todo: Warn if it took too long to process samples
        println!(
            "Warning: Took too long to process samples: {}ms",
            elapsed_millis
        );
    }

    let corrected = duration_micros
        .checked_sub(elapsed_micros)
        .unwrap_or_default();

    spin_sleep::sleep(Duration::from_micros(corrected as u64));
}
