use dashmap::DashMap;
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tokio::runtime::Handle;

mod player;
mod timeline;

pub use player::*;
pub use timeline::*;

use crate::{Config, Ingestion, Output, Sink};

/// The playback type is responsible for managing players, processing playback, and preloading sinks as needed.
pub struct Playback {
    config: Config,
    output: Arc<Output>,
    /// All the players that eixst in the playback.
    players: Arc<DashMap<PlayerId, Player>>,
}

impl Playback {
    pub fn new<I>(rt: Handle, config: Config, ingestion: Arc<I>, output: Arc<Output>) -> Self
    where
        I: Ingestion + 'static,
    {
        let players = Arc::new(DashMap::new());

        spawn_procesing_thread(config.clone(), players.clone());
        spawn_preloading_task(rt, config.clone(), ingestion, players.clone());

        Self {
            output,
            config,
            players,
        }
    }

    pub fn set_sinks(&self, player_id: PlayerId, sinks: Vec<Arc<Sink>>) {
        if let Some(player) = self.players.get(&player_id) {
            player.set_sinks(sinks);
        }
    }

    /// Creates a new player, registers it with the output, and returns its id.
    pub fn create_player(&self) -> PlayerId {
        let player = Player::new(self.config.clone(), self.output.clone());
        self.output.register_player(player.id);

        player.id
    }
}

fn spawn_procesing_thread(config: Config, players: Arc<DashMap<PlayerId, Player>>) {
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
    handle: Handle,
    config: Config,
    ingestion: Arc<I>,
    players: Arc<DashMap<PlayerId, Player>>,
) where
    I: Ingestion + 'static,
{
    handle.spawn(async move {
        for player in players.iter() {
            let preload = player.preload();

            if let Some(preload) = preload {
                ingestion
                    .request_load(
                        preload.sink_id,
                        preload.offset,
                        config.preload_size_in_samples(),
                    )
                    .await;
            }
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
    }

    let corrected = duration_micros
        .checked_sub(elapsed_micros)
        .unwrap_or_default();

    spin_sleep::sleep(Duration::from_micros(corrected as u64));
}
