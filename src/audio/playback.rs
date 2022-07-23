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
    loader: Arc<Loader>,
    // TODO: This data is duplicated, perhaps find a way to deal with that
    expected: AtomicCell<usize>,
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
        let queue = self.queue.lock().unwrap();

        let result: Vec<_> = queue
            .iter()
            .scan((amount, self.offset.load()), |(remaining, offset), item| {
                if *remaining == 0 {
                    return None;
                }

                let available = item.available.load();
                let amount_ahead = available.checked_sub(*offset).unwrap_or_default();

                // Don't read more than requested
                let amount_to_read = amount_ahead.min(*remaining);
                let read_range = *offset..(*offset + amount_to_read);

                *remaining -= amount_to_read;
                let result = (item.loader.id(), read_range);

                // This item is not finished loading, so stop here
                if !item.complete() {
                    *remaining = 0;
                }

                self.offset.store(*offset + amount_to_read);
                *offset = 0;

                Some(result)
            })
            .collect();

        let total_read = result.iter().map(|(_, r)| r.len()).sum();
        self.total_offset.fetch_add(total_read);

        result
    }

    /// Returns the a vec containing loaders to load data for
    /// If there is no need to load, it returns no items
    pub fn preload(&self) -> Vec<(LoaderId, usize)> {
        let available = self.total_available.load() - self.offset.load();

        if available > PRELOAD_THRESHOLD {
            return vec![];
        }

        let queue = self.queue.lock().unwrap();

        queue
            .iter()
            .skip_while(|i| i.complete())
            .scan(PRELOAD_AMOUNT, |remaining, item| {
                let unloaded = item.expected.load() - item.available.load();
                let amount_to_load = unloaded.min(*remaining);

                if *remaining > 0 {
                    *remaining -= amount_to_load;
                    Some((item.loader.id(), amount_to_load))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn set_loaders(&self, new_loaders: Vec<Arc<Loader>>) {
        {
            let mut queue = self.queue.lock().unwrap();
            *queue = new_loaders.into_iter().map(ScheduledItem::new).collect();
        }

        self.calculate_total_available()
    }

    /// Called when a loader has more content
    pub fn notify_load(&self, id: LoaderId, new_amount: usize) {
        {
            let queue = self.queue.lock().unwrap();
            queue.iter().for_each(|i| i.update());
        }

        self.calculate_total_available()
    }

    fn calculate_total_available(&self) {
        let queue = self.queue.lock().unwrap();

        let result = queue
            .iter()
            .scan(true, |previous_was_complete, item| {
                if !*previous_was_complete {
                    return None;
                }

                let available = item.available.load();
                *previous_was_complete = item.complete();

                Some(available)
            })
            .sum();

        self.total_available.store(result);
    }
}

impl ScheduledItem {
    fn new(loader: Arc<Loader>) -> Self {
        let me = Self {
            loader,
            expected: 0.into(),
            available: 0.into(),
        };

        me.update();
        me
    }

    fn complete(&self) -> bool {
        self.available.load() == self.expected.load()
    }

    fn update(&self) {
        self.available.store(self.loader.available());
        self.expected.store(self.loader.expected());
    }
}
