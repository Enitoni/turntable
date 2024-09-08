use std::{sync::Arc, thread};

use crate::{Config, PipelineContext, PlayerId, Sample};
use crossbeam::channel::{unbounded, Receiver, Sender};
use dashmap::DashMap;

mod consumer;
mod encoder;
mod stream;

pub use consumer::*;
pub use encoder::*;
use log::info;
pub use stream::*;

/// Manages streams for consuming a [Player].
pub struct Output {
    config: Config,
    streams: Arc<DashMap<PlayerId, Arc<Stream>>>,
    sample_sender: Sender<ProcessedSamples>,
}

struct ProcessedSamples {
    player_id: PlayerId,
    samples: Vec<Sample>,
}

impl Output {
    pub fn new(context: &PipelineContext) -> Self {
        let (sample_sender, sample_receiver) = unbounded();
        let streams = Arc::new(DashMap::new());

        spawn_output_thread(sample_receiver, streams.clone());

        Self {
            config: context.config.clone(),
            streams,
            sample_sender,
        }
    }

    /// Creates a new stream for the given player.
    pub fn register_player(&self, player_id: PlayerId) {
        let new_stream = Stream::new(self.config.clone());
        self.streams.insert(player_id, new_stream);
    }

    /// Gets a consumer for the associated player, with the given encoder.
    pub fn consume_player<E>(&self, player_id: PlayerId, with_latency: Option<u32>) -> Consumer
    where
        E: Encoder,
    {
        let stream = self
            .streams
            .get(&player_id)
            .expect("consume_player() is not called with a player that does not exist");

        let consumer = stream.consume::<E>(with_latency);

        info!(
            "Created {} consumer #{} of player #{}",
            E::name(),
            consumer.id,
            player_id,
        );

        consumer
    }

    /// Pushes samples to the associated player's stream.
    pub fn push(&self, player_id: PlayerId, samples: Vec<Sample>) {
        self.sample_sender
            .send(ProcessedSamples { player_id, samples })
            .expect("processed samples are sent");
    }
}

fn spawn_output_thread(
    receiver: Receiver<ProcessedSamples>,
    streams: Arc<DashMap<PlayerId, Arc<Stream>>>,
) {
    let run = move || loop {
        let processed_samples = receiver.recv().expect("processed samples are received");

        if let Some(stream) = streams.get(&processed_samples.player_id) {
            stream.push(&processed_samples.samples);
        }
    };

    thread::Builder::new()
        .name("output".to_string())
        .spawn(run)
        .expect("output thread is spawned");
}
