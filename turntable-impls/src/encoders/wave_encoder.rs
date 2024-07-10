use turntable_core::{Config, Encoder, Sample};

/// Encodes [Sample]s into a .wav file
pub struct WaveEncoder {
    did_write_header: bool,
    encoded_bytes: Vec<u8>,
    header: WaveHeader,
}

#[derive(Debug, Clone, Copy)]
enum WaveHeaderValue {
    Ascii(&'static str),
    TwoBytes(u16),
    FourBytes(u32),
}

#[derive(Debug, Clone)]
struct WaveHeader {
    channel_count: u16,
    sample_rate: u32,
    bit_depth: u16,
}

impl WaveHeaderValue {
    fn to_bytes(self) -> Vec<u8> {
        match self {
            WaveHeaderValue::Ascii(x) => x.as_bytes().to_vec(),
            WaveHeaderValue::TwoBytes(x) => x.to_le_bytes().to_vec(),
            WaveHeaderValue::FourBytes(x) => x.to_le_bytes().to_vec(),
        }
    }
}

impl WaveHeader {
    // ChunkID: Contains the letters "RIFF" in ASCII form, change last number to 80 if "RIFX" is used
    const CHUNK_ID: WaveHeaderValue = WaveHeaderValue::Ascii("RIFF");

    // This is set to max because it's a live audio stream.
    const CHUNK_SIZE: WaveHeaderValue = WaveHeaderValue::FourBytes(i32::MAX as u32);

    // Format: Contains the letters "WAVE"
    const FORMAT: WaveHeaderValue = WaveHeaderValue::Ascii("WAVE");

    // Subchunk1ID: Contains the letters "fmt "
    const FMT_CHUNK_ID: WaveHeaderValue = WaveHeaderValue::Ascii("fmt ");

    // Subchunk1Size: 16 for PCM.
    const FMT_CHUNK_SIZE: WaveHeaderValue = WaveHeaderValue::FourBytes(16);

    // AudioFormat: PCM = 1
    const AUDIO_FORMAT: WaveHeaderValue = WaveHeaderValue::TwoBytes(1);

    // Subchunk2ID: Contains the letters "data"
    const DATA_CHUNK_ID: WaveHeaderValue = WaveHeaderValue::Ascii("data");

    fn to_bytes(&self) -> Vec<u8> {
        let num_channels = WaveHeaderValue::TwoBytes(self.channel_count);
        let sample_rate = WaveHeaderValue::FourBytes(self.sample_rate);

        let byte_rate = WaveHeaderValue::FourBytes(
            self.sample_rate * self.channel_count as u32 * self.bit_depth as u32 / 8,
        );

        let block_align = WaveHeaderValue::TwoBytes(self.channel_count * self.bit_depth / 8);
        let bits_per_sample = WaveHeaderValue::TwoBytes(self.bit_depth);

        // This is set to max because turntable is a live audio stream
        let data_chunk_size = WaveHeaderValue::FourBytes(i32::MAX as u32);

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
        .flat_map(WaveHeaderValue::to_bytes)
        .collect()
    }
}

impl Encoder for WaveEncoder {
    fn new(config: Config) -> Self
    where
        Self: Sized,
    {
        let header = WaveHeader {
            channel_count: config.channel_count as u16,
            sample_rate: config.sample_rate as u32,
            bit_depth: 16,
        };

        Self {
            did_write_header: false,
            encoded_bytes: Vec::new(),
            header,
        }
    }

    fn content_type(&self) -> String {
        "audio/wav".to_string()
    }

    fn name() -> String
    where
        Self: Sized,
    {
        "WaveEncoder".to_string()
    }

    fn encode(&mut self, samples: &[Sample]) {
        let samples_in_bytes: Vec<_> = samples
            .iter()
            .map(|s| (s * i16::MAX as Sample) as i16)
            .flat_map(|s| s.to_le_bytes())
            .collect();

        self.encoded_bytes.extend_from_slice(&samples_in_bytes);
    }

    fn bytes(&mut self) -> Option<Vec<u8>> {
        let mut bytes = vec![];

        // Return nothing until there's data available
        if self.encoded_bytes.is_empty() {
            return None;
        }

        if !self.did_write_header {
            let header_bytes = self.header.to_bytes();
            bytes.extend_from_slice(&header_bytes);

            self.did_write_header = true;
        }

        bytes.extend_from_slice(&self.encoded_bytes);
        self.encoded_bytes.truncate(0);

        Some(bytes)
    }
}
