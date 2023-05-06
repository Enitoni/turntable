use serde::Serialize;

use crate::ingest::Sink;

/// A playable audio track, which can be queued.
/// It may provide metadata as well.
#[derive(Clone, Debug, Serialize)]
pub struct Track {
    #[serde(skip)]
    pub sink: Sink,

    /// This is temporary
    pub title: String,
    pub duration: f32,
}

impl Track {
    pub fn new(duration: f32, title: String, sink: Sink) -> Self {
        Self {
            duration,
            title,
            sink,
        }
    }
}
