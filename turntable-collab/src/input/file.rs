use async_trait::async_trait;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use regex::Regex;
use tokio::fs::File;
use turntable_core::{BoxedLoadable, Loadable};
use turntable_impls::LoadableFile;

use crate::{InputError, Inputable};

lazy_static! {
    static ref REGEX: Regex = Regex::new(r"^file://([a-zA-Z0-9_/\\:-]+\.[a-zA-Z0-9_]+)$").unwrap();
}

// A file that can be played by turntable.
pub struct FileInput {
    file: Mutex<Option<File>>,
}

#[async_trait]
impl Inputable for FileInput {
    fn test(query: &str) -> bool {
        REGEX.is_match(query)
    }

    async fn fetch(query: &str) -> Result<Vec<Self>, InputError>
    where
        Self: Sized,
    {
        let path = REGEX
            .captures(query)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .ok_or(InputError::Invalid)?;

        let file = File::open(path)
            .await
            .map_err(|e| InputError::Other(e.to_string()))?;

        Ok(vec![Self {
            file: Mutex::new(Some(file)),
        }])
    }

    fn length(&self) -> Option<f32> {
        None
    }

    async fn loadable(&self) -> Result<BoxedLoadable, InputError> {
        let file = self.file.lock().take().expect("file is taken");
        let boxed = LoadableFile::new(file).boxed();

        Ok(boxed)
    }
}
