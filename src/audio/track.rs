use std::sync::Arc;

use super::{Input, Loader, LoaderId};

/// A playable audio track, which can be queued.
/// It may provide metadata as well.
#[derive(Clone)]
pub struct Track {
    pub loader: Arc<Loader>,
}

impl Track {
    pub fn new(loader: Arc<Loader>) -> Self {
        Self { loader }
    }
}
