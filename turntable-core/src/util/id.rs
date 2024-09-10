use std::fmt::{Debug, Display};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use crossbeam::atomic::AtomicCell;

pub type IdType = u64;
pub static ID_COUNTER: AtomicCell<IdType> = AtomicCell::new(1);

/// A unique identifier for any type.
pub struct Id<T> {
    value: IdType,
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

    pub fn value(&self) -> u64 {
        self.value
    }
}

impl<T> Default for Id<T> {
    fn default() -> Self {
        Self::none()
    }
}

impl<T> Debug for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl<T> Display for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state)
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Id<T> {}
impl<T> Eq for Id<T> {}
