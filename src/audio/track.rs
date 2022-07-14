use super::{source::FileSource, AudioSource};

/// A playable audio track, which can be queued.
/// It may provide metadata as well.
#[derive(Debug, Clone)]
pub struct Track {
    source: TrackSource,
}

impl Track {
    pub fn new<T: Into<TrackSource>>(source: T) -> Self {
        let source = source.into();
        Self { source }
    }

    pub fn source(&self) -> &TrackSource {
        &self.source
    }
}

#[derive(Debug, Clone)]
pub enum TrackSource {
    File(FileSource),
}

impl TrackSource {
    fn as_trait_mut(&mut self) -> &mut dyn AudioSource {
        match self {
            TrackSource::File(x) => x,
        }
    }

    fn as_trait(&self) -> &dyn AudioSource {
        match self {
            TrackSource::File(x) => x,
        }
    }
}

impl AudioSource for TrackSource {
    fn id(&self) -> super::source::SourceId {
        self.as_trait().id()
    }

    fn length(&self) -> usize {
        self.as_trait().length()
    }

    fn read_samples(
        &mut self,
        offset: usize,
        buf: &mut [super::Sample],
    ) -> Result<usize, super::source::Error> {
        self.as_trait_mut().read_samples(offset, buf)
    }
}

impl From<FileSource> for TrackSource {
    fn from(source: FileSource) -> Self {
        TrackSource::File(source.into())
    }
}
