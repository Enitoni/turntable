use std::fmt::Debug;

use super::{new::StreamConsumer, Sample, SAMPLE_IN_BYTES};
use std::io::Read;

/// Implements streaming a .wav file
pub struct WaveStream {
    underlying: StreamConsumer,
    did_write_header: bool,
    header: WaveHeader,
}

#[derive(Debug, Clone, Copy)]
enum HeaderValue {
    Ascii(&'static str),
    TwoBytes(u16),
    FourBytes(u32),
}

struct WaveHeader {
    channel_count: u16,
    sample_rate: u32,
    bit_depth: u16,
}

impl WaveStream {
    pub const MIME: &'static str = "audio/wav";

    pub fn new(underlying: StreamConsumer) -> Self {
        let header = WaveHeader {
            channel_count: super::CHANNEL_COUNT as u16,
            sample_rate: super::SAMPLE_RATE as u32,
            bit_depth: 16,
        };

        Self {
            header,
            underlying,
            did_write_header: false,
        }
    }
}

impl Debug for WaveStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WaveStream")
    }
}

impl Read for WaveStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let header = self.header.to_bytes();
        let header_len = header.len();

        let mut bytes_written = 0;

        if !self.did_write_header {
            buf[..header_len].copy_from_slice(&header);

            bytes_written = header_len;
            self.did_write_header = true;
        }

        let body_buf = &mut buf[bytes_written..];

        let mut samples = vec![0f32; body_buf.len() / (SAMPLE_IN_BYTES / 2)];
        let amount_of_samples = self.underlying.read(&mut samples);

        let samples_in_bytes: Vec<_> = samples[..amount_of_samples]
            .iter()
            .map(|s| (s * i16::MAX as Sample) as i16)
            .flat_map(|s| s.to_le_bytes())
            .collect();

        body_buf[..samples_in_bytes.len()].copy_from_slice(&samples_in_bytes);
        bytes_written += samples_in_bytes.len();

        Ok(bytes_written)
    }
}

impl HeaderValue {
    fn to_bytes(self) -> Vec<u8> {
        match self {
            HeaderValue::Ascii(x) => x.as_bytes().to_vec(),
            HeaderValue::TwoBytes(x) => x.to_le_bytes().to_vec(),
            HeaderValue::FourBytes(x) => x.to_le_bytes().to_vec(),
        }
    }
}

impl WaveHeader {
    // ChunkID: Contains the letters "RIFF" in ASCII form, change last number to 80 if "RIFX" is used
    const CHUNK_ID: HeaderValue = HeaderValue::Ascii("RIFF");

    // This is set to max because vinyl is a live audio stream
    const CHUNK_SIZE: HeaderValue = HeaderValue::FourBytes(i32::MAX as u32);

    // Format: Contains the letters "WAVE"
    const FORMAT: HeaderValue = HeaderValue::Ascii("WAVE");

    // Subchunk1ID: Contains the letters "fmt "
    const FMT_CHUNK_ID: HeaderValue = HeaderValue::Ascii("fmt ");

    // Subchunk1Size: 16 for PCM.
    const FMT_CHUNK_SIZE: HeaderValue = HeaderValue::FourBytes(16);

    // AudioFormat: PCM = 1
    const AUDIO_FORMAT: HeaderValue = HeaderValue::TwoBytes(1);

    // Subchunk2ID: Contains the letters "data"
    const DATA_CHUNK_ID: HeaderValue = HeaderValue::Ascii("data");

    fn to_bytes(&self) -> Vec<u8> {
        let num_channels = HeaderValue::TwoBytes(self.channel_count);
        let sample_rate = HeaderValue::FourBytes(self.sample_rate);

        let byte_rate = HeaderValue::FourBytes(
            self.sample_rate * self.channel_count as u32 * self.bit_depth as u32 / 8,
        );

        let block_align = HeaderValue::TwoBytes(self.channel_count * self.bit_depth / 8);
        let bits_per_sample = HeaderValue::TwoBytes(self.bit_depth);

        // This is set to max because vinyl is a live audio stream
        let data_chunk_size = HeaderValue::FourBytes(i32::MAX as u32);

        [
            Self::CHUNK_ID,
            Self::CHUNK_SIZE,
            Self::FORMAT,
            Self::FMT_CHUNK_ID,
            Self::FMT_CHUNK_SIZE,
            Self::AUDIO_FORMAT,
            num_channels,
            sample_rate,
            byte_rate,
            block_align,
            bits_per_sample,
            Self::DATA_CHUNK_ID,
            data_chunk_size,
        ]
        .into_iter()
        .flat_map(HeaderValue::to_bytes)
        .collect()
    }
}
