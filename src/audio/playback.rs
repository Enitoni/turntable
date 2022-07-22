use super::{Loader, LoaderId, PRELOAD_AMOUNT, PRELOAD_THRESHOLD};
use crate::util::model::Identified;
use crossbeam::atomic::AtomicCell;
use std::{
    ops::Range,
    sync::{Arc, Mutex},
};

/// Schedules loading and playback for a list of loaders
pub struct Scheduler {
    /// The loaders in the queue, the first one is the current
    queue: Mutex<Vec<ScheduledItem>>,
    /// Current playback offset of the current loader
    offset: AtomicCell<usize>,
    /// Total amount of playback time
    total_offset: AtomicCell<usize>,
    /// Amount of contiguous loaded samples
    total_available: AtomicCell<usize>,
}

struct ScheduledItem {
    loader: LoaderId,
    // TODO: This data is duplicated, perhaps find a way to deal with that
    expected: usize,
    available: AtomicCell<usize>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            queue: Default::default(),
            offset: Default::default(),
            total_offset: Default::default(),
            total_available: Default::default(),
        }
    }

    /// Returns a list of advancements describing loaders to read from,
    /// If this returns more than 1 item, it signifies that one or more
    /// loaders have been played all the way through.
    pub fn advance(&self, amount: usize) -> Vec<(LoaderId, Range<usize>)> {
        let mut result = vec![];
        let mut read = 0;

        let queue = self.queue.lock().unwrap();

        for (i, item) in queue.iter().enumerate() {
            if i > 0 {
                self.offset.store(0);
            }

            let offset = self.offset.load();

            let available = item.available.load();
            let remaining = available.checked_sub(offset).unwrap_or_default();

            let amount_to_read = remaining.min(amount - read);

            read += amount_to_read;
            result.push((item.loader, offset..(offset + amount_to_read)));

            //dbg!(read, amount);
            self.offset.fetch_add(amount_to_read);
        }

        self.total_offset.fetch_add(read);
        result
    }

    /// Returns the a vec containing loaders to load data for
    pub fn preload(&self) -> Vec<(LoaderId, usize)> {
        let available = self.total_available.load() - self.offset.load();

        if available > PRELOAD_THRESHOLD {
            return vec![];
        }

        let queue = self.queue.lock().unwrap();

        queue
            .iter()
            .skip_while(|i| i.complete())
            .scan(PRELOAD_AMOUNT, |mut remaining, item| {
                let unloaded = item.expected - item.available.load();
                let amount_to_load = unloaded.min(*remaining);

                if *remaining > 0 {
                    *remaining -= amount_to_load;
                    Some((item.loader, amount_to_load))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Called when the active queue updates
    pub fn handle_queue_update(&self, new_loaders: Vec<Arc<Loader>>) {
        let mut queue = self.queue.lock().unwrap();

        *queue = new_loaders
            .into_iter()
            .map(|l| ScheduledItem {
                loader: l.id(),
                expected: l.expected(),
                available: l.available().into(),
            })
            .collect();
    }

    /// Called when a loader has more content
    pub fn handle_load(&self, id: LoaderId, new_amount: usize) {
        {
            let queue = self.queue.lock().unwrap();

            queue
                .iter()
                .find(|s| s.loader == id)
                .map(|s| s.available.store(new_amount))
                .unwrap_or(());
        }

        self.total_available.store(self.calculate_total_available());
    }

    fn calculate_total_available(&self) -> usize {
        let queue = self.queue.lock().unwrap();

        queue
            .iter()
            .enumerate()
            .map(|(i, x)| (i, x.available.load(), x.expected))
            .take_while(|(i, available, expected)| *i == 0 || available == expected)
            .fold(0, |acc, (_, x, _)| acc + x)
    }
}

impl ScheduledItem {
    fn complete(&self) -> bool {
        self.available.load() == self.expected
    }
}
