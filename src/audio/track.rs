use super::{source::FileSource, AudioSource, ToAudioSource};

/// A playable audio track, which can be queued.
/// It may provide metadata as well.
pub struct Track {
    source: TrackSource,
}

impl Track {
    pub fn new<T: Into<TrackSource>>(source: T) -> Self {
        let source = source.into();
        Self { source }
    }
}

pub enum TrackSource {
    File(FileSource),
}

impl From<FileSource> for TrackSource {
    fn from(source: FileSource) -> Self {
        TrackSource::File(source.into())
    }
}
