use std::{error::Error, io::SeekFrom};

use async_trait::async_trait;
use crossbeam::atomic::AtomicCell;
use reqwest::Client;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
    sync::Mutex,
};

use crate::{IntoLoadable, Loadable, LoaderLength, ReadResult};
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

    pub async fn new(url: String) -> Result<Self, reqwest::Error> {
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
            let range = format!("bytes={start}-{end}");
            request = request.header("Range", range);
        }

        let mut response = request.send().await?;
        let mut amount_read = 0;

        while let Some(chunk) = response.chunk().await? {
            let buf_to_write = &mut buf[amount_read..];
            buf_to_write.copy_from_slice(&chunk[..buf_to_write.len()]);

            amount_read += chunk.len();
        }

        let result = if amount_read == amount {
            ReadResult::More(amount_read)
        } else {
            ReadResult::End(amount_read)
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
            SeekFrom::End(_) => {
                return Err("Seeking from the end is not supported".into());
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

    pub fn test_file_path() -> PathBuf {
        let root = env!("CARGO_MANIFEST_DIR");
        let mut path = PathBuf::from(root);

        path.pop();
        path.push("assets");
        path.push("deep_blue.flac");

        path
    }

    pub async fn test_file() -> File {
        File::open(test_file_path()).await.unwrap()
    }
}
