use std::{fmt::Debug, ops::Range};

/// Returns a safe range that will not overflow
pub fn safe_range(length: usize, range: Range<usize>) -> Range<usize> {
    let start = range.start.min(length - 1);
    let end = range.end.max(start).min(length - 1);

    start..end
}

pub fn ranges_overlap<T>(a: &Range<T>, b: &Range<T>) -> bool
where
    T: PartialOrd + Ord + Clone,
{
    a.contains(&b.start) || b.contains(&a.end)
}

pub fn combine_two_ranges<T>(a: &Range<T>, b: &Range<T>) -> Range<T>
where
    T: Copy + PartialOrd + Ord,
{
    let start = a.start.min(b.start);
    let end = a.end.max(b.end);

    start..end
}

pub fn merge_ranges<T>(ranges: Vec<Range<T>>) -> Vec<Range<T>>
where
    T: PartialOrd + Ord + Clone + Copy + Debug,
{
    let mut results = vec![];
    let mut current = ranges.first().cloned().unwrap();

    for (i, range) in ranges.iter().enumerate() {
        if ranges_overlap(&current, range) {
            current = combine_two_ranges(&current, range);
        } else {
            results.push(current);
            current = range.clone();
        }

        if i == ranges.len() - 1 {
            results.push(current.clone());
        }
    }

    results
}

#[cfg(test)]
mod test {
    use super::merge_ranges;

    #[test]
    fn merge_ranges_works() {
        let ranges_to_merge = vec![0..4, 3..7, 7..35, 6..90];
        let merged = merge_ranges(ranges_to_merge);
        assert_eq!(merged, vec![0..90]);

        let ranges_to_merge = vec![0..4, 2..3, 7..35, 6..90];
        let merged = merge_ranges(ranges_to_merge);
        assert_eq!(merged, vec![0..4, 6..90]);
    }
}

/// Utilities for modeling data types.
pub mod model {
    use std::{
        collections::BTreeMap,
        fmt::Display,
        marker::PhantomData,
        ops::Deref,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc, Mutex, MutexGuard, Weak,
        },
    };

    use log::warn;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// A unique id to identify a type.
    #[derive(Debug)]
    pub struct Id<T>(u64, PhantomData<T>);

    impl<T> Id<T> {
        pub fn new() -> Id<T>
        where
            T: Identified,
        {
            Self(COUNTER.fetch_add(1, Ordering::Relaxed), PhantomData)
        }
    }

    impl<T> Copy for Id<T> {}
    impl<T> Clone for Id<T> {
        fn clone(&self) -> Self {
            Id(self.0, PhantomData)
        }
    }

    impl<T> Eq for Id<T> {}
    impl<T> PartialEq for Id<T> {
        fn eq(&self, other: &Self) -> bool {
            self.0.eq(&other.0)
        }
    }

    impl<T> Ord for Id<T> {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.0.cmp(&other)
        }
    }

    impl<T> PartialOrd for Id<T> {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.0.cmp(&other.0))
        }
    }

    impl<T> Deref for Id<T> {
        type Target = u64;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> Default for Id<T>
    where
        T: Identified,
    {
        fn default() -> Self {
            Self::new()
        }
    }

    /// This type can be identified
    pub trait Identified
    where
        Self: Sized,
    {
        const NAME: &'static str;

        fn id(&self) -> Id<Self>;
    }

    impl<T> Display for Id<T>
    where
        T: Identified,
    {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{} {}", T::NAME, self.0)
        }
    }

    /// A thread-safe general purpose store for long-lived types.
    pub struct Store<T> {
        items: Mutex<BTreeMap<Id<T>, Arc<T>>>,
    }

    impl<T> Store<T>
    where
        T: Identified,
    {
        pub fn new() -> Self {
            Self {
                items: Default::default(),
            }
        }

        /// Insert a new item into the store.
        ///
        /// This will panic if the item already exists,
        /// to ensure inserts are always intentional.
        pub fn insert(&self, item: T) -> Id<T> {
            let mut items = self.items_guard();
            let id = item.id();

            #[cfg(debug_assertions)]
            if items.contains_key(&id) {
                warn!("Id {} for {} is not unique", id, T::NAME)
            }

            items.insert(id, Arc::new(item));
            id
        }

        /// Get a weak reference to an item in the store by id, if it exists
        pub fn get(&self, id: Id<T>) -> Option<Arc<T>> {
            self.items_guard().get(&id).cloned()
        }

        /// Get a weak reference to an item in the store by id.
        /// This will panic if the item does not exist in the store.
        pub fn get_expect(&self, id: Id<T>) -> Arc<T> {
            self.get(id)
                .unwrap_or_else(|| panic!("{} with id {} does not exist", T::NAME, id))
        }

        /// Get a vec of optional references to an item.
        pub fn get_many<I: IntoIterator<Item = Id<T>>>(&self, ids: I) -> Vec<Option<Arc<T>>> {
            let items = self.items_guard();

            ids.into_iter().map(|id| items.get(&id).cloned()).collect()
        }

        /// Get a vec of references to an item-
        /// This will panic if any item does not exist in the store.
        pub fn get_many_expect<I: IntoIterator<Item = Id<T>>>(&self, ids: I) -> Vec<Arc<T>> {
            let items = self.items_guard();

            ids.into_iter()
                .map(|id| (id, items.get(&id)))
                .map(|(id, opt)| {
                    opt.cloned().unwrap_or_else(|| {
                        panic!("{} with id {} does not exist in ids", T::NAME, id)
                    })
                })
                .collect()
        }

        /// Removes the item for the store, and returns it if it exists.
        /// This will panic if there are any `Arc<T>` elsewhere of this item.
        pub fn remove(&self, id: Id<T>) -> Option<T> {
            self.items_guard().remove(&id).map(|i| {
                Arc::try_unwrap(i).ok().unwrap_or_else(|| {
                    panic!(
                        "Expected only one strong reference to Arc<{}> of id {}",
                        T::NAME,
                        id
                    )
                })
            })
        }

        /// Removes the item for the store and returns it.
        /// This will panic if the item does not exist.
        pub fn remove_expect(&self, id: Id<T>) -> T {
            self.remove(id)
                .unwrap_or_else(|| panic!("{} with id {} to remove does not exist", T::NAME, id))
        }

        /// Removes an item from the store and drops it.
        /// This will panic if the item does not exist.
        pub fn delete(&self, id: Id<T>) {
            self.items_guard()
                .remove(&id)
                .unwrap_or_else(|| panic!("{} with id {} to delete does not exist", T::NAME, id));
        }

        fn items_guard(&self) -> MutexGuard<BTreeMap<Id<T>, Arc<T>>> {
            self.items
                .lock()
                .unwrap_or_else(|_| panic!("Items for store of {} is poisoned", T::NAME))
        }
    }
}
