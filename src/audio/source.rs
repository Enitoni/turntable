use super::Sample;
pub type SourceId = u64;

#[derive(Debug, Clone)]
pub enum Error {
    /// The source could not provide samples.
    Read { reason: String, retry: bool },
}

impl Error {
    /// Returns true if the source should be skipped.
    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::Read { retry: false, .. })
    }
}

/// Represents a source of audio.
///
/// Though caching strategies are fine, this will probably cloned
/// and therefore any state should only serve the purpose of providing samples.
pub trait AudioSource: 'static + Send + Sync + AudioSourceClone {
    /// How many samples does this source return
    fn length(&self) -> usize;

    /// Returns a key that is converted to an id
    fn id(&self) -> SourceId;

    /// Reads the samples into the provided buffer, returning how many
    /// samples were read, or an error if something went wrong.
    ///
    /// This function may be blocking.
    fn read_samples(&mut self, offset: usize, buf: &mut [Sample]) -> Result<usize, Error>;
}

pub trait AudioSourceClone {
    fn clone_box(&self) -> Box<dyn AudioSource>;
}

impl<T> AudioSourceClone for T
where
    T: 'static + AudioSource + Clone,
{
    fn clone_box(&self) -> Box<dyn AudioSource> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn AudioSource> {
    fn clone(&self) -> Box<dyn AudioSource> {
        self.clone_box()
    }
}

mod file {
    use std::{
        collections::hash_map::DefaultHasher,
        fs::{File, Metadata},
        hash::{Hash, Hasher},
        io::{Read, Seek, SeekFrom},
        path::PathBuf,
    };

    use super::{AudioSource, Error};
    use crate::audio::{decoding::raw_samples_from_bytes, Sample, SourceId, SAMPLE_IN_BYTES};

    /// A local audio file
    #[derive(Debug, Clone)]
    pub struct FileSource {
        path: PathBuf,
    }

    impl FileSource {
        pub fn new<P: Into<PathBuf>>(path: P) -> Self {
            Self { path: path.into() }
        }

        fn file(&self) -> File {
            File::open(&self.path).expect("File can be opened")
        }

        fn meta(&self, file: &File) -> Metadata {
            file.metadata().expect("Metadata can be read")
        }

        fn fingerprint(&self) -> String {
            let file = self.file();
            let metadata = self.meta(&file);

            let name = self
                .path
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default();

            format!(
                "__{name}:{:?}:{:?}",
                metadata.file_type(),
                metadata.created()
            )
        }
    }

    impl AudioSource for FileSource {
        fn id(&self) -> SourceId {
            let mut hasher = DefaultHasher::default();
            let fingerprint = self.fingerprint();

            fingerprint.hash(&mut hasher);
            hasher.finish()
        }

        fn length(&self) -> usize {
            let file = self.file();
            let meta = self.meta(&file);

            meta.len() as usize / SAMPLE_IN_BYTES
        }

        fn read_samples(&mut self, offset: usize, buf: &mut [Sample]) -> Result<usize, Error> {
            let mut file = self.file();

            file.seek(SeekFrom::Start(offset as u64))
                .map_err(|e| Error::Read {
                    reason: e.to_string(),
                    retry: false,
                })?;

            let mut raw_buf = vec![0; buf.len() * SAMPLE_IN_BYTES];

            file.read(&mut raw_buf).map_err(|e| Error::Read {
                reason: e.to_string(),
                retry: false,
            })?;

            let samples = raw_samples_from_bytes(&raw_buf);
            buf[..samples.len()].copy_from_slice(&samples);

            Ok(samples.len())
        }
    }
}

pub use file::*;
