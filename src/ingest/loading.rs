use std::fmt::Debug;

use super::sink::SinkLength;

#[derive(Debug)]
pub enum LoadResult {
    Data(Vec<u8>),
    Empty,
    Error,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ProbeResult {
    pub length: SinkLength,
}

pub trait Loader
where
    Self: 'static + Sync + Send + Debug,
{
    fn load(&mut self, amount: usize) -> LoadResult;
    fn probe(&self) -> Option<ProbeResult>;
}
