use std::marker::PhantomData;

use crossbeam::atomic::AtomicCell;

pub static ID_COUNTER: AtomicCell<u64> = AtomicCell::new(1);

/// A unique identifier for any type.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Id<T> {
    value: u64,
    kind: PhantomData<T>,
}

impl<T> Id<T> {
    /// Creates a new id.
    pub fn new() -> Self {
        Self {
            value: ID_COUNTER.fetch_add(1),
            kind: PhantomData,
        }
    }

    /// Returns an empty id.
    pub fn none() -> Self {
        Self {
            value: 0,
            kind: PhantomData,
        }
    }
}

impl<T> Default for Id<T> {
    fn default() -> Self {
        Self::none()
    }
}
