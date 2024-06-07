use std::sync::Arc;

use crate::{Config, Id, Output, Sink, Timeline, TimelinePreload};

pub type PlayerId = Id<Player>;

/// The player is responsible for managing the playback of a [Timeline],
/// and writing the played samples to an output buffer.
pub struct Player {
    pub id: PlayerId,
    config: Config,
    timeline: Timeline,
    output: Arc<Output>,
}

impl Player {
    pub fn new(config: Config, output: Arc<Output>) -> Self {
        Self {
            timeline: Timeline::new(config.clone()),
            id: PlayerId::new(),
            output,
            config,
        }
    }

    pub fn preload(&self) -> Vec<TimelinePreload> {
        self.timeline.preload()
    }

    pub fn set_sinks(&self, sinks: Vec<Arc<Sink>>) {
        self.timeline.set_sinks(sinks);
    }

    /// Processes the timeline and pushes the samples to the output stream.
    pub fn process(&self) {
        let mut samples = vec![0.; self.config.buffer_size_in_samples()];
        let mut amount_read = 0;

        let reads = self.timeline.advance(samples.len());

        for read in reads {
            let slice = &mut samples[amount_read..];
            let result = read.sink.read(read.offset, slice);

            amount_read += result.amount;
        }

        self.output.push(self.id, samples);
    }

    /// Clears samples that are not needed, to save memory.
    pub fn clear_superflous(&self) {
        self.timeline.clear_superflous();
    }
}
