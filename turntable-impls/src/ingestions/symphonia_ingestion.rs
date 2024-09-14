use async_trait::async_trait;
use crossbeam::atomic::AtomicCell;
use parking_lot::Mutex;
use rubato::{FftFixedInOut, Resampler};
use std::{
    error::Error,
    io::{ErrorKind as IoErrorKind, Read, Seek, SeekFrom},
};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{Decoder, CODEC_TYPE_NULL},
    errors::Error as SymphoniaError,
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo, Track},
    io::{MediaSource, MediaSourceStream},
    meta::MetadataOptions,
    probe::Hint,
    units::Time,
};
use tokio::runtime::Handle;

use turntable_core::{
    get_or_create_handle, BoxedLoadable, Config, Ingest, Ingestion, IntoLoadable, LoadRequest,
    Loadable, LoaderLength, PipelineContext, ReadResult, Sample, WriteGuard,
};

type SymphoniaResampler = FftFixedInOut<Sample>;

/// An ingestion implementation for Symphonia.
pub struct SymphoniaIngestion {
    /// A runtime is needed to bridge synchronous Symphonia with asynchronous turntable.
    rt: Handle,
    context: PipelineContext,
    format_options: FormatOptions,
}

#[async_trait]
impl Ingestion for SymphoniaIngestion {
    type Loader = Loader;

    fn new(context: &PipelineContext) -> Self {
        Self {
            rt: get_or_create_handle(),
            context: context.clone(),
            format_options: FormatOptions {
                enable_gapless: true,
                prebuild_seek_index: false,
                seek_index_fill_rate: 20,
            },
        }
    }

    async fn ingest<L>(&self, input: L) -> Result<Ingest<Self::Loader>, Box<dyn Error>>
    where
        L: IntoLoadable + Send + Sync,
    {
        let input = input.into_loadable();

        input.activate().await?;

        let potential_sink_length = input
            .length()
            .await
            .and_then(|l| l.to_sink_length(self.context.config.clone()));

        let loadable = LoadableMediaSource {
            rt: self.rt.clone(),
            loadable: input.boxed(),
        };

        let stream = MediaSourceStream::new(Box::new(loadable), Default::default());
        let format_options = self.format_options;
        let probed = self
            .rt
            .spawn_blocking(move || {
                symphonia::default::get_probe().format(
                    &Hint::default(),
                    stream,
                    &format_options,
                    &MetadataOptions::default(),
                )
            })
            .await??;

        let format_reader = probed.format;
        let audio_track = format_reader
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or("Symphonia: No supported audio stream found")?;

        let sample_rate = audio_track
            .codec_params
            .sample_rate
            .map(|s| s as usize)
            .unwrap_or(self.context.config.sample_rate);

        let resampler = DynamicResampler::new(sample_rate, &self.context.config)?;

        let codec_params = audio_track.codec_params.clone();
        let decoder = self
            .rt
            .spawn_blocking(move || {
                symphonia::default::get_codecs().make(&codec_params, &Default::default())
            })
            .await??;

        // Get the decoded length of the audio track, if possible.
        let potential_decoded_seconds = audio_track
            .codec_params
            .time_base
            .and_then(|time_base| {
                audio_track
                    .codec_params
                    .n_frames
                    .map(|n_frames| time_base.calc_time(n_frames))
            })
            .map(|time| time.seconds as f32 + time.frac as f32);

        // Prefer symphonia's decoded length over the sink length.
        // If neither is available, the sink will be treated as infinite.
        let sink_length = potential_decoded_seconds
            .map(|s| self.context.config.seconds_to_samples(s))
            .or(potential_sink_length);

        let loader = Loader {
            decoder: decoder.into(),
            track: audio_track.clone(),
            offset: Default::default(),
            resampler: resampler.into(),
            config: self.context.config.clone(),
            format_reader: format_reader.into(),
        };

        Ok(Ingest {
            expected_length: sink_length,
            loader,
        })
    }

    async fn request_load(&self, request: LoadRequest<Self::Loader>) {
        let _ = self
            .rt
            .spawn_blocking(move || {
                request
                    .loader
                    .load(request.write_guard, request.offset, request.amount)
            })
            .await;
    }

    fn name() -> String {
        "Symphonia".to_string()
    }
}

/// Loads the samples from a [Decoder] into a [Sink].
pub struct Loader {
    config: Config,
    track: Track,
    offset: AtomicCell<usize>,
    decoder: Mutex<Box<dyn Decoder>>,
    format_reader: Mutex<Box<dyn FormatReader>>,
    resampler: Mutex<DynamicResampler>,
}

impl Loader {
    fn load(&self, guard: WriteGuard, offset: usize, amount: usize) -> Result<(), ()> {
        let result = self.load_into_sink(offset, amount, &guard);

        match result {
            Ok(result) => {
                if result.end_reached {
                    guard.end()
                }
            }
            Err(e) => {
                guard.error(format!("{:?}", e));
                return Err(());
            }
        }

        Ok(())
    }

    // Loads the samples into the sink.
    fn load_into_sink(
        &self,
        offset: usize,
        amount: usize,
        write_ref: &WriteGuard,
    ) -> Result<LoadResult, Box<dyn Error>> {
        let old_offset = self.offset.load();
        let mut seeked_offset = offset;

        if old_offset != offset {
            seeked_offset = self.seek(offset)?;
        }

        let result = self.decode_until_filled(amount)?;

        // Skip the seek difference, to avoid artifacts.
        let start = offset.saturating_sub(seeked_offset);
        let samples = &result.samples[start..];

        write_ref.write(offset, samples);
        self.offset.store(old_offset + result.samples.len());

        Ok(result)
    }

    // Attempts to seek to the given offset.
    fn seek(&self, offset: usize) -> Result<usize, Box<dyn Error>> {
        let mut format_reader = self.format_reader.lock();

        let samples_in_seconds = self.config.samples_to_seconds(offset);
        let time = Time {
            frac: samples_in_seconds.fract() as f64,
            seconds: samples_in_seconds.trunc() as u64,
        };

        let seeked_to = format_reader.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time,
                track_id: Some(self.track.id),
            },
        )?;

        let time = self
            .track
            .codec_params
            .time_base
            .expect("this is not none if seek was successful")
            .calc_time(seeked_to.actual_ts);

        let seeked_to_seconds = time.seconds as f32 + time.frac as f32;
        let seeked_to_offset = self.config.seconds_to_samples(seeked_to_seconds);

        self.offset.store(seeked_to_offset);
        Ok(seeked_to_offset)
    }

    // Decode the amount of samples requested.
    // Note: More samples may be returned than requested.
    fn decode_until_filled(&self, amount: usize) -> Result<LoadResult, Box<dyn Error>> {
        let mut last_samples_written_was_zero = false;
        let mut end_reached = false;

        let mut decoder = self.decoder.lock();
        let mut format_reader = self.format_reader.lock();

        let mut samples = vec![];

        loop {
            if samples.len() >= amount {
                break;
            }

            let packet = match format_reader.next_packet() {
                Ok(packet) => Ok(packet),
                // Assume the end of the stream.
                Err(SymphoniaError::IoError(err)) => {
                    if err.kind() == IoErrorKind::UnexpectedEof {
                        end_reached = true;
                        break;
                    }

                    return Err(err.into());
                }
                Err(e) => Err(e),
            }?;

            if packet.track_id() != self.track.id {
                continue;
            }

            match decoder.decode(&packet) {
                Ok(decoded) => {
                    let mut sample_buffer =
                        SampleBuffer::<Sample>::new(decoded.capacity() as u64, *decoded.spec());

                    // Acquire the samples from the decoder.
                    sample_buffer.copy_interleaved_ref(decoded);
                    let decoded_samples = sample_buffer.samples();

                    // Sometimes Symphonia does not err with UnexpectedEof, but instead returns no samples.
                    // This is a workaround to avoid an infinite loop.
                    if last_samples_written_was_zero && samples.is_empty() {
                        end_reached = true;
                        break;
                    }

                    // Copy the samples into the buffer.
                    samples.extend_from_slice(decoded_samples);
                    last_samples_written_was_zero = samples.is_empty();
                }
                Err(SymphoniaError::IoError(err)) => {
                    if err.kind() == IoErrorKind::UnexpectedEof {
                        end_reached = true;
                        break;
                    }

                    return Err(err.into());
                }
                Err(SymphoniaError::DecodeError(_)) => {
                    // The packet failed to decode due to invalid data, skip the packet.
                    continue;
                }
                // Handle unknown errors.
                Err(err) => {
                    println!("unexpected error: {:?}", &err);
                    return Err(err.into());
                }
            }
        }

        let mut resampler = self.resampler.lock();
        let samples = resampler.process(samples);

        Ok(LoadResult {
            samples,
            end_reached,
        })
    }
}

/// Uninterleaves a chunk of samples into a vector where each sub-vector is a channel.
fn uninterleave_samples(samples: Vec<Sample>, channels: usize) -> Vec<Vec<Sample>> {
    let mut uninterleaved_samples = vec![];
    let chunks: Vec<Vec<f32>> = samples.chunks_exact(channels).map(|c| c.to_vec()).collect();

    for c in 0..channels {
        let mut channel_samples = vec![];

        for chunk in chunks.iter() {
            channel_samples.push(chunk[c]);
        }

        uninterleaved_samples.push(channel_samples);
    }

    uninterleaved_samples
}

/// Interleaves vectors of channels into a single vector of samples
fn interleave_samples(samples: Vec<Vec<Sample>>) -> Vec<Sample> {
    if samples.is_empty() {
        return vec![];
    }
    let channel_len = samples[0].len();
    assert!(samples.iter().all(|channel| channel.len() == channel_len));

    (0..channel_len)
        .flat_map(|sample_index| samples.iter().map(move |channel| channel[sample_index]))
        .collect()
}

#[derive(Debug)]
struct LoadResult {
    samples: Vec<Sample>,
    end_reached: bool,
}

/// Bridges an async [Loadable] with a synchronous [MediaSource].
struct LoadableMediaSource {
    rt: Handle,
    loadable: BoxedLoadable,
}

impl MediaSource for LoadableMediaSource {
    fn is_seekable(&self) -> bool {
        self.rt.block_on(self.loadable.seekable())
    }

    fn byte_len(&self) -> Option<u64> {
        let result = self.rt.block_on(self.loadable.length());

        result.and_then(|l| match l {
            LoaderLength::Bytes(bytes) => Some(bytes as u64),
            LoaderLength::Time(_) => None,
        })
    }
}

impl Seek for LoadableMediaSource {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let result = self.rt.block_on(self.loadable.seek(pos));

        result
            .map_err(|e| std::io::Error::other(format!("Seek failed: {:?}", e)))
            .map(|seek| seek as u64)
    }
}

impl Read for LoadableMediaSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let result = self.rt.block_on(self.loadable.read(buf));

        result
            .map_err(|e| std::io::Error::other(format!("Read failed: {:?}", e)))
            .map(|read| match read {
                ReadResult::More(bytes) => bytes,
                ReadResult::End(bytes) => bytes,
            })
    }
}

/// A resampler that can take any length of samples as input
struct DynamicResampler {
    resampler: SymphoniaResampler,
    channel_count: usize,
    source_sample_rate: usize,
    target_sample_rate: usize,
    did_remove_silence: bool,
}

impl DynamicResampler {
    const CHUNK_SIZE: usize = 1024;

    fn new(source_sample_rate: usize, config: &Config) -> Result<Self, Box<dyn Error>> {
        let resampler = SymphoniaResampler::new(
            source_sample_rate,
            config.sample_rate,
            Self::CHUNK_SIZE,
            config.channel_count,
        )?;

        Ok(Self {
            resampler,
            source_sample_rate,
            channel_count: config.channel_count,
            target_sample_rate: config.sample_rate,
            did_remove_silence: false,
        })
    }

    fn process(&mut self, samples: Vec<Sample>) -> Vec<Sample> {
        // Don't do anything if it's not necessary
        if self.target_sample_rate == self.source_sample_rate {
            return samples;
        }

        let mut interleaved_result = vec![0f32; 0];
        let chunked_channels: Vec<_> = uninterleave_samples(samples, self.channel_count)
            .into_iter()
            .map(|c| {
                c.chunks_exact(Self::CHUNK_SIZE)
                    .map(|c| c.to_vec())
                    .collect::<Vec<_>>()
            })
            .collect();

        let chunk_amount = chunked_channels[0].len();

        for chunk_index in 0..chunk_amount {
            let mut chunks = vec![];

            // ok clippy
            (0..self.channel_count).for_each(|channel_index| {
                let channel_chunk = &chunked_channels[channel_index][chunk_index];
                chunks.push(channel_chunk.to_owned());
            });

            let resampled = self
                .resampler
                .process(&chunks, None)
                .expect("processes without issue");

            let interleaved = interleave_samples(resampled);
            interleaved_result.extend_from_slice(&interleaved)
        }

        if !self.did_remove_silence {
            self.did_remove_silence = true;

            let silence_in_samples = self.resampler.output_delay() * self.channel_count;

            return interleaved_result
                .into_iter()
                .skip(silence_in_samples)
                .collect();
        }

        interleaved_result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uninterleave_samples() {
        let samples = vec![1., 2., 3., 4., 5., 6.];
        let result = uninterleave_samples(samples, 2);

        assert_eq!(result, vec![vec![1., 3., 5.], vec![2., 4., 6.]])
    }

    #[test]
    fn test_interleave_samples() {
        let samples = vec![vec![1., 3., 5.], vec![2., 4., 6.]];
        let result = interleave_samples(samples);

        assert_eq!(result, vec![1., 2., 3., 4., 5., 6.]);
    }
}
