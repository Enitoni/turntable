use std::{error::Error, io::SeekFrom};

use async_trait::async_trait;
use crossbeam::atomic::AtomicCell;
use reqwest::Client;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
    sync::Mutex,
};

use crate::{assign_slice, IntoLoadable, Loadable, LoaderLength, ReadResult};
pub struct LoadableFile(Mutex<File>);

#[async_trait]
impl Loadable for LoadableFile {
    async fn read(&self, buf: &mut [u8]) -> Result<ReadResult, Box<dyn Error>> {
        let mut file = self.0.lock().await;
        let result = file.read(buf).await?;

        if result == 0 {
            return Ok(ReadResult::End(0));
        }

        Ok(ReadResult::More(result))
    }

    async fn length(&self) -> Option<LoaderLength> {
        let file = self.0.lock().await;

        file.metadata()
            .await
            .ok()
            .map(|meta| meta.len())
            .map(|length| LoaderLength::Bytes(length as usize))
    }

    async fn seek(&self, seek: SeekFrom) -> Result<usize, Box<dyn Error>> {
        let mut file = self.0.lock().await;
        let result = file.seek(seek).await?;

        Ok(result as usize)
    }
}

impl IntoLoadable for File {
    type Output = LoadableFile;

    fn into_loadable(self) -> Self::Output {
        LoadableFile(Mutex::new(self))
    }
}

/// A loadable that reads from a network stream.
/// If the stream supports byte ranges, it can be seeked.
pub struct LoadableNetworkStream {
    url: String,
    client: Client,
    length: Option<usize>,
    offset: AtomicCell<usize>,
    supports_byte_ranges: bool,
}

impl LoadableNetworkStream {
    const MAX_CHUNK_SIZE: usize = 500000;

    pub async fn new<S>(url: S) -> Result<Self, reqwest::Error>
    where
        S: Into<String>,
    {
        let url = url.into();
        let client = Client::new();

        let response = client.head(&url).send().await?;
        let headers = response.headers();

        let supports_byte_ranges = headers
            .get("Accept-Ranges")
            .map(|v| v == "bytes")
            .unwrap_or_default();

        let length = headers
            .get("Content-Length")
            .map(|v| str::parse::<usize>(v.to_str().unwrap()).unwrap_or_default());

        Ok(Self {
            url,
            client,
            length,
            offset: Default::default(),
            supports_byte_ranges,
        })
    }
}

#[async_trait]
impl Loadable for LoadableNetworkStream {
    async fn read(&self, buf: &mut [u8]) -> Result<ReadResult, Box<dyn Error>> {
        let amount = buf.len().min(Self::MAX_CHUNK_SIZE);

        let start = self.offset.load();
        let end = (start + amount).min(self.length.unwrap_or(usize::MAX));

        let mut request = self.client.get(&self.url);

        if self.supports_byte_ranges {
            let safe_end = end.saturating_sub(1);
            let range = format!("bytes={start}-{safe_end}");
            request = request.header("Range", range);
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(format!("Request failed with status code {}", response.status()).into());
        }

        let bytes = response.bytes().await?;
        let amount_written = assign_slice(&bytes, buf);

        let result = if amount_written == amount {
            ReadResult::More(amount_written)
        } else {
            ReadResult::End(amount_written)
        };

        Ok(result)
    }

    async fn length(&self) -> Option<LoaderLength> {
        self.length.map(LoaderLength::Bytes)
    }

    async fn seek(&self, seek: SeekFrom) -> Result<usize, Box<dyn Error>> {
        let offset = self.offset.load();

        let new_offset = match seek {
            SeekFrom::Current(increment) => (increment + offset as i64) as usize,
            SeekFrom::Start(offset) => offset as usize,
            SeekFrom::End(increment) => {
                (self.length.unwrap_or(usize::MAX) as i64 + increment) as usize
            }
        };

        self.offset.store(new_offset);
        Ok(new_offset)
    }
}

#[cfg(test)]
pub mod tests {
    use std::path::PathBuf;

    use tokio::fs::File;

    pub fn test_file_path(file: &str) -> PathBuf {
        let root = env!("CARGO_MANIFEST_DIR");
        let mut path = PathBuf::from(root);

        path.pop();
        path.push("assets");
        path.push(file);

        path
    }

    pub async fn test_file(file: &str) -> File {
        File::open(test_file_path(file)).await.unwrap()
    }
}
