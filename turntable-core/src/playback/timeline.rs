use std::sync::Arc;

use crossbeam::atomic::AtomicCell;
use parking_lot::Mutex;

use crate::{Config, Sink, SinkId};

/// The timeline keeps track of a sequence of sinks, manages advancement of playback, and returns what sinks to preload.
#[derive(Debug, Default)]
pub struct Timeline {
    config: Config,
    /// A sequence of sinks. The first one is the currently playing one.
    sinks: Mutex<Vec<Arc<Sink>>>,
    /// The playback offset of the first sink.
    offset: AtomicCell<usize>,
    /// The total playback offset of the timeline.
    total_offset: AtomicCell<usize>,
}

impl Timeline {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            sinks: Default::default(),
            offset: Default::default(),
            total_offset: Default::default(),
        }
    }

    /// Sets the sinks to play and preload.
    ///
    /// Calling this function will not reset the playback offset to 0 if the first sink is different from the current one.
    /// This is because the player may have already started reading from the next sink, and we don't want to reset the offset.
    ///
    /// Instead, this function is meant to be called when we advance to the next sink, so that future sinks in a queue can be preloaded.
    pub fn set_sinks(&self, sinks: Vec<Arc<Sink>>) {
        *self.sinks.lock() = sinks;
    }

    /// Advances the playback offset and returns the sinks that the player should read from.
    /// If this returns more than one [TimelineRead], it means the current sink finished playing.
    ///
    /// However, this cannot be relied upon, because it is possible for the remaining samples to be 0 exactly when moving on to the next sink.
    /// In that case the vector will still only be one item, since it breaks before pushing the next [TimelineRead].
    /// Therefore, the player should instead check if the [SinkId] changed to determine if the next sink started playing.
    ///
    /// If the returned vector is empty, it means the player is at the end of the timeline.
    ///
    /// * `amount` - The requested amount of samples.
    pub fn advance(&self, amount: usize) -> Vec<TimelineRead> {
        let mut result = vec![];

        let playable_sinks: Vec<_> = self
            .sinks
            .lock()
            .iter()
            .filter(|s| s.is_playable())
            .cloned()
            .collect();

        let mut remaining = amount;
        let mut offset = self.offset.load();

        for sink in playable_sinks {
            // We've satisified the amount of samples the player wants to play
            if remaining == 0 {
                break;
            }

            let available_until_void = sink.distance_from_void(offset);
            let amount_to_read = available_until_void.distance.min(remaining);
            let new_offset = offset + amount_to_read;

            // There are samples to read from this sink.
            if amount_to_read > 0 {
                remaining -= amount_to_read;
                result.push(TimelineRead {
                    offset,
                    sink: sink.clone(),
                    amount: amount_to_read,
                });

                self.total_offset.fetch_add(amount_to_read);
                self.offset.store(new_offset);
            }

            // Reset offset for the next sink.
            offset = 0;

            // Let's break down the conditions for moving on to the next sink.
            // 1. We've reached the end of the last loaded range of samples, and
            // 2. The sink is sealed/not loadable, meaning there won't be any more samples to load, and
            // 3. There are no more remaining samples to read.
            let should_move_on = !sink.is_loadable()
                && available_until_void.is_end
                && available_until_void.distance.saturating_sub(amount_to_read) == 0;

            // Stop here if we're not moving on to the next sink.
            if !should_move_on {
                break;
            }
        }

        result
    }

    /// Returns what sinks to preload, if any.
    ///
    /// This should never be called while a sink is being loaded.
    pub fn preload(&self) -> Vec<TimelinePreload> {
        let sinks: Vec<_> = self.sinks.lock().iter().cloned().collect();

        let threshold = self.config.preload_threshold_in_samples();
        let mut offset = self.offset.load();

        let mut result = vec![];

        for sink in sinks {
            let available_until_void = sink.distance_from_void(offset);

            if available_until_void.distance < threshold && sink.is_loadable() {
                result.push({
                    TimelinePreload {
                        sink_id: sink.id,
                        offset,
                    }
                });
            }

            // If we're not at the end of the sink, we should stop here.
            if !available_until_void.is_end {
                break;
            }

            // Set the preload offset to the start for the next sink.
            offset = 0;
        }

        result
    }

    /// Returns the offset of the current sink.
    pub fn current_offset(&self) -> usize {
        self.offset.load()
    }

    /// Returns the total offset of the timeline.
    pub fn total_offset(&self) -> usize {
        self.total_offset.load()
    }
}

/// Instructs a [Player] what sink to read from, and where to start reading from.
#[derive(Debug)]
pub struct TimelineRead {
    pub sink: Arc<Sink>,
    /// The offset of the first sample to read from the sink.
    pub offset: usize,
    /// How many samples to read from the offset.
    pub amount: usize,
}

/// Instructs [Playback] what sinks to preload.
#[derive(Debug)]
pub struct TimelinePreload {
    pub sink_id: SinkId,
    // The offset in samples to start preloading from.
    pub offset: usize,
}
