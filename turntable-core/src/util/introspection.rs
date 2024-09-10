use std::fmt::Debug;

/// Implementors are expected to return a dummy object with all public fields.
pub trait Introspect<T>
where
    T: Debug,
{
    fn introspect(&self) -> T;
}

impl<I, O> Introspect<Vec<O>> for Vec<I>
where
    I: Introspect<O>,
    O: Debug,
{
    fn introspect(&self) -> Vec<O> {
        self.iter().map(|x| x.introspect()).collect()
    }
}
