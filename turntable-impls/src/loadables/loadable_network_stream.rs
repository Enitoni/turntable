use std::{error::Error, io::SeekFrom};

use async_trait::async_trait;
use crossbeam::atomic::AtomicCell;
use parking_lot::Mutex;
use reqwest::Client;
use turntable_core::{assign_slice, Loadable, LoaderLength, ReadResult};

/// A loadable that reads from a network stream.
/// If the stream supports byte ranges, it can be seeked.
pub struct LoadableNetworkStream {
    url: String,
    client: Client,
    length: Mutex<Option<usize>>,
    supports_byte_ranges: AtomicCell<bool>,
    read_offset: AtomicCell<usize>,
    /// A cache of bytes that are preloaded to prevent spamming the network.
    /// This is because the ingestion may request very small amounts of data at a time.
    loaded_bytes: Mutex<Vec<u8>>,
    loaded_bytes_offset: AtomicCell<usize>,
}

impl LoadableNetworkStream {
    const MAX_CHUNK_SIZE: usize = 50_000_000; // 50MB
    const MIN_CHUNK_SIZE: usize = 3_000_000; // 3MB

    pub fn new<S>(url: S) -> Result<Self, Box<dyn Error>>
    where
        S: Into<String>,
    {
        let url = url.into();
        let client = Client::new();

        Ok(Self {
            url,
            client,
            length: Default::default(),
            supports_byte_ranges: Default::default(),
            read_offset: Default::default(),
            loaded_bytes: Default::default(),
            loaded_bytes_offset: Default::default(),
        })
    }

    async fn setup(&self) -> Result<(), Box<dyn Error>> {
        let response = self.client.head(&self.url).send().await?;
        let status = response.status();
        let headers = response.headers();

        if !status.is_success() {
            return Err(format!("Stream initialization failed with {}", status).into());
        }

        let supports_byte_ranges = headers
            .get("Accept-Ranges")
            .map(|v| v == "bytes")
            .unwrap_or_default();

        let length = headers
            .get("Content-Length")
            .map(|v| str::parse::<usize>(v.to_str().unwrap()).unwrap_or_default());

        self.supports_byte_ranges.store(supports_byte_ranges);
        *self.length.lock() = length;

        Ok(())
    }

    /// Loads an amount of bytes from the current load offset.
    async fn load(&self, amount: usize) -> Result<(), Box<dyn Error>> {
        // Set up the offsets that we will load from and to.
        let start = self.loaded_bytes_offset.load();
        let end = (start + amount).min(self.normal_len()).saturating_sub(1);

        let mut request = self.client.get(&self.url);

        if self.supports_byte_ranges.load() {
            let range = format!("bytes={}-{}", start, end);
            request = request.header("Range", range);
        }

        let response = request.send().await?;
        let status = response.status();

        if !status.is_success() {
            return Err(format!("Stream load failed with {}", status).into());
        }

        let new_bytes = response.bytes().await?;
        let mut current_bytes = self.loaded_bytes.lock();

        current_bytes.extend_from_slice(&new_bytes);
        self.loaded_bytes_offset.store(end + 1);

        Ok(())
    }

    /// Ensures that the given amount of bytes at the given offset are preloaded from the network.
    async fn ensure_preloaded(&self, requested_amount: usize) -> Result<(), Box<dyn Error>> {
        let current_read_offset = self.read_offset.load();
        let current_load_offset = self.loaded_bytes_offset.load();
        let current_loaded_len = self.loaded_bytes.lock().len();

        // This number is relative to the vector
        let new_read_offset = current_read_offset + requested_amount;
        let remaining_bytes = self
            .length
            .lock()
            .unwrap_or(usize::MAX)
            .saturating_sub(current_load_offset);

        // Skip preloading if we have enough data for the read, or if there's no more data to read
        if new_read_offset <= current_loaded_len || remaining_bytes == 0 {
            return Ok(());
        }

        // Preload at least MIN_CHUNK_SIZE if possible, but not more than the remaining bytes
        let minimum_requested_amount = requested_amount
            .max(Self::MIN_CHUNK_SIZE)
            .min(remaining_bytes);

        self.load(minimum_requested_amount).await?;
        Ok(())
    }

    fn normal_len(&self) -> usize {
        self.length.lock().unwrap_or(usize::MAX)
    }

    fn loaded_length(&self) -> usize {
        self.loaded_bytes.lock().len()
    }

    fn absolute_load_start_offset(&self) -> usize {
        self.loaded_bytes_offset
            .load()
            .saturating_sub(self.loaded_length())
    }

    fn absolute_read_offset(&self) -> usize {
        self.absolute_load_start_offset() + self.read_offset.load()
    }
}

#[async_trait]
impl Loadable for LoadableNetworkStream {
    async fn read(&self, buf: &mut [u8]) -> Result<ReadResult, Box<dyn Error>> {
        let amount_to_read = buf.len().min(Self::MAX_CHUNK_SIZE);

        self.ensure_preloaded(amount_to_read).await?;

        let loaded_bytes = self.loaded_bytes.lock();
        let read_offset = self.read_offset.load();

        let safe_read_start = read_offset.min(loaded_bytes.len());
        let read_bytes = &loaded_bytes[safe_read_start..];

        let amount_written = assign_slice(read_bytes, buf);
        let new_read_offset = read_offset + amount_written;

        self.read_offset.store(new_read_offset);

        let result = if amount_written == buf.len() {
            ReadResult::More(amount_written)
        } else {
            ReadResult::End(amount_written)
        };

        Ok(result)
    }

    async fn length(&self) -> Option<LoaderLength> {
        self.length.lock().map(LoaderLength::Bytes)
    }

    async fn seek(&self, seek: SeekFrom) -> Result<usize, Box<dyn Error>> {
        let absolute_read_offset = self.absolute_read_offset();
        let absolute_load_start_offset = self.absolute_load_start_offset();
        let absolute_load_end_offset = self.loaded_bytes_offset.load();

        let new_absolute_read_offset = match seek {
            SeekFrom::Current(increment) => (absolute_read_offset as i64 + increment) as usize,
            SeekFrom::Start(offset) => offset as usize,
            SeekFrom::End(increment) => (self.normal_len() as i64 + increment) as usize,
        };

        let safe_new_offset = new_absolute_read_offset.min(self.normal_len());
        let safe_new_relative_read_offset =
            new_absolute_read_offset.saturating_sub(absolute_load_start_offset);

        // Reset the loaded bytes if the seek is outside the current loaded range.
        if new_absolute_read_offset < absolute_load_start_offset
            || new_absolute_read_offset > absolute_load_end_offset
        {
            self.read_offset.store(0);
            self.loaded_bytes.lock().clear();
            self.loaded_bytes_offset.store(safe_new_offset);
        } else {
            self.read_offset.store(safe_new_relative_read_offset);
        }

        Ok(safe_new_offset)
    }
}
