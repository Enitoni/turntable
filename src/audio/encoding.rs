use std::fmt::Debug;

use super::AudioBufferConsumer;
use std::io::Read;

/// Implements streaming a .wav file
pub struct WaveStream {
    underlying: AudioBufferConsumer,
    did_write_header: bool,
}

impl WaveStream {
    /// WAVE file header.
    /// Specifies 2 channel interleaved 32-bit floating point
    const HEADER: [u8; 44] = [
        /*
        // ChunkID: Contains the letters "RIFF" in ASCII form, change last number to 80 if "RIFX" is used
        82, 73, 70, 70,
        // ChunkSize: 36 + SubChunk2Size
        36, 0, 0, 0,
        // Format: Contains the letters "WAVE"
        87, 65, 86, 69,
        // Subchunk1ID: Contains the letters "fmt "
        102, 109, 116, 32,
        // Subchunk1Size: 16 for PCM.
        16, 0, 0, 0,
        // AudioFormat: PCM = 1
        1, 0,
        // NumChannels: Mono = 1, Stereo = 2, etc.
        2, 0,
        // SampleRate: 8000, 44100, etc.
        68, 212, 0, 0,
        // ByteRate: SampleRate * NumChannels * BitsPerSample/8
        32, 98, 5, 0,
        // BlockAlign: == NumChannels * BitsPerSample / 8
        8, 0,
        // BitsPerSample: 8 bits = 8, 16 bits = 16, etc.
        32, 0,
        // Subchunk2ID: Contains the letters "data"
        100, 97, 116, 97,
        // Subchunk2Size: NumSamples * NumChannels * BitsPerSample / 8
        0, 0, 0, 0
        */
        82, 73, 70, 70, 36, 0, 0, 0, 87, 65, 86, 69, 102, 109, 116, 32, 16, 0, 0, 0, 3, 0, 2, 0, 68,
        172, 0, 0, 32, 98, 5, 0, 8, 0, 32, 0, 100, 97, 116, 97, 0, 0, 0, 0,
    ];

    pub const MIME: &'static str = "audio/wav";

    pub fn new(underlying: AudioBufferConsumer) -> Self {
        Self {
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
        let header = Self::HEADER;
        let header_len = header.len();

        let mut bytes_written = 0;

        if !self.did_write_header {
            buf[..header_len].copy_from_slice(&header);

            bytes_written = header_len;
            self.did_write_header = true;
        }

        bytes_written += self.underlying.read(&mut buf[bytes_written..])?;
        Ok(bytes_written)
    }
}
