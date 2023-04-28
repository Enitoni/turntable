use crate::ingest::Sink;

/// A playable audio track, which can be queued.
/// It may provide metadata as well.
#[derive(Clone)]
pub struct Track {
    pub sink: Sink,
}

impl Track {
    pub fn new(sink: Sink) -> Self {
        Self { sink }
    }
}
