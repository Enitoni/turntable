use crossbeam::atomic::AtomicCell;
use parking_lot::Mutex;

use crate::ingest::{Sink, SinkId};

use super::PRELOAD_THRESHOLD;

/// A list of consecutive sinks that keeps track of offset and amount loaded.
///
/// This is responsible for advancing the playback.
#[derive(Debug, Default)]
pub struct Timeline {
    /// A sequence of sinks. The first one is the currently playing one.
    pub(super) sinks: Mutex<Vec<Sink>>,
    /// Offset of the current sink.
    pub(super) offset: AtomicCell<usize>,
    /// The total amount of samples that have been advanced.
    pub(super) total_offset: AtomicCell<usize>,
}

impl Timeline {
    pub fn set_sinks(&self, sinks: Vec<Sink>) {
        *self.sinks.lock() = sinks
    }

    /// Optionally returns a sink to preload if necessary
    pub fn preload(&self) -> Option<SinkId> {
        let sinks: Vec<_> = self.sinks.lock().iter().cloned().collect();

        let total_available: usize = sinks
            .iter()
            .scan(true, |previous_was_complete, item| {
                if !*previous_was_complete {
                    return None;
                }

                let available = item.available();
                *previous_was_complete = item.is_complete();

                Some(available)
            })
            .sum();

        let available = total_available.saturating_sub(self.offset.load());

        if available > PRELOAD_THRESHOLD {
            return None;
        }

        sinks
            .into_iter()
            .filter(|s| !s.is_complete())
            .take(1)
            .find_map(|s| (!s.is_pending()).then(|| s.id()))
    }

    /// Advance the timeline and return a list of advancements describing sinks to read from.
    pub fn advance(&self, amount: usize) -> Vec<Advancement> {
        let mut result = vec![];

        let sinks: Vec<_> = self
            .sinks
            .lock()
            .iter()
            // Avoid playing the same sink when it advances
            .filter(|s| !s.is_consumed())
            .cloned()
            .collect();

        let mut remaining = amount;
        let mut offset = self.offset.load();

        for sink in sinks {
            if remaining == 0 {
                break;
            }

            let available = sink.available();
            let amount_ahead = available.saturating_sub(offset);
            let amount_to_read = amount_ahead.min(remaining);
            let new_offset = offset + amount_to_read;

            remaining -= amount_to_read;
            result.push(Advancement {
                sink: sink.clone(),
                start_offset: offset,
                end_offset: new_offset,
            });

            self.total_offset.fetch_add(amount_to_read);
            self.offset.store(new_offset);

            offset = 0;

            if !sink.is_complete() {
                break;
            }
        }

        result
    }
}

#[derive(Debug)]
/// Read from a sink at a specific offset.
pub struct Advancement {
    pub(super) sink: Sink,
    pub(super) start_offset: usize,
    pub(super) end_offset: usize,
}
