use reqwest::blocking::Client;
use std::{
    io::{ErrorKind, Read},
    sync::atomic::{AtomicUsize, Ordering},
};

/// An HTTP stream that can be retried in case of network errors
#[derive(Debug)]
pub struct ByteRangeStream {
    url: String,
    client: Client,
    length: usize,
    offset: AtomicUsize,
}

impl ByteRangeStream {
    const MAX_CHUNK_SIZE: usize = 500000;

    /// Tries to create a byte range stream from the provided URL,
    /// returning [None] if the endpoint does not support byte ranges.
    pub fn try_new(url: String) -> Option<Self> {
        let client = Client::new();
        let response = client.head(&url).send();

        match response {
            Ok(res) => {
                let headers = res.headers();

                let range_header = headers
                    .get("Accept-Ranges")
                    .map(|v| v == "bytes")
                    .unwrap_or_default();

                if !range_header {
                    return None;
                }

                let length = headers
                    .get("Content-Length")
                    .map(|v| str::parse::<usize>(v.to_str().unwrap()).unwrap_or_default())
                    .unwrap_or_default();

                Some(Self {
                    url,
                    client,
                    length,
                    offset: 0.into(),
                })
            }
            Err(_) => None,
        }
    }

    pub fn remaining(&self) -> usize {
        self.length
            .saturating_sub(self.offset.load(Ordering::Relaxed))
    }
}

impl Read for ByteRangeStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let requested_amount = buf.len().min(Self::MAX_CHUNK_SIZE).min(self.remaining());

        let start = self.offset.load(Ordering::Relaxed);
        let end = start + requested_amount;

        let range = format!("bytes={start}-{end}");

        let mut response = self
            .client
            .get(&self.url)
            .header("Range", range)
            .send()
            .map_err(|_| std::io::Error::from(ErrorKind::Other))?;

        response.read_exact(&mut buf[..requested_amount])?;
        self.offset.fetch_add(requested_amount, Ordering::Relaxed);

        Ok(requested_amount)
    }
}
