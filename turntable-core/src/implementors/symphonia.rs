use async_trait::async_trait;
use crossbeam::atomic::AtomicCell;
use dashmap::DashMap;
use parking_lot::Mutex;
use std::{
    error::Error,
    io::{ErrorKind as IoErrorKind, Read, Seek, SeekFrom},
    sync::Arc,
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
use tokio::runtime::{self, Handle};

use crate::{
    BoxedLoadable, Config, Ingestion, IntoLoadable, Loadable, LoaderLength, ReadResult, Sample,
    Sink, SinkId, SinkState,
};

/// An ingestion implementation for Symphonia.
pub struct SymphoniaIngestion {
    /// A runtime is needed to bridge synchronous Symphonia with asynchronous turntable.
    rt: Handle,
    config: Config,
    sinks: DashMap<SinkId, Arc<Sink>>,
    loaders: DashMap<SinkId, Arc<Loader>>,
    format_options: FormatOptions,
}

#[async_trait]
impl Ingestion for SymphoniaIngestion {
    fn new(config: Config) -> Self {
        Self {
            rt: runtime::Handle::current(),
            config,
            sinks: DashMap::new(),
            loaders: DashMap::new(),
            format_options: FormatOptions {
                enable_gapless: true,
                prebuild_seek_index: false,
                seek_index_fill_rate: 20,
            },
        }
    }

    async fn ingest<L>(&self, input: L) -> Result<Arc<Sink>, Box<dyn Error>>
    where
        L: IntoLoadable + Send + Sync,
    {
        let input = input.into_loadable();

        let potential_sink_length = input
            .length()
            .await
            .and_then(|l| l.to_sink_length(self.config.clone()));

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
            .map(|s| self.config.seconds_to_samples(s))
            .or(potential_sink_length);

        let sink: Arc<_> = Sink::new(sink_length).into();

        let loader = Loader {
            sink: sink.clone(),
            decoder: decoder.into(),
            track: audio_track.clone(),
            offset: Default::default(),
            config: self.config.clone(),
            format_reader: format_reader.into(),
        };

        self.loaders.insert(sink.id, loader.into());
        self.sinks.insert(sink.id, sink.clone());

        Ok(sink)
    }

    async fn request_load(&self, sink_id: SinkId, offset: usize, amount: usize) {
        let loader = self.loaders.get(&sink_id).expect("loader exists").clone();

        let _ = self
            .rt
            .spawn_blocking(move || loader.load(offset, amount))
            .await;
    }

    fn clear_inactive(&self) {
        self.sinks.retain(|_, s| !s.is_clearable());
    }
}

/// Loads the samples from a [Decoder] into a [Sink].
struct Loader {
    config: Config,
    track: Track,
    sink: Arc<Sink>,
    offset: AtomicCell<usize>,
    decoder: Mutex<Box<dyn Decoder>>,
    format_reader: Mutex<Box<dyn FormatReader>>,
}

impl Loader {
    fn load(&self, offset: usize, amount: usize) -> Result<(), ()> {
        self.sink.set_state(SinkState::Loading);
        let result = self.load_into_sink(offset, amount);

        match result {
            Ok(result) => {
                if result.end_reached {
                    self.sink.set_state(SinkState::Sealed);
                } else {
                    // Todo: Deal with this in a better way. This has to be set to `Active` to avoid the player skipping it.
                    // It is a bad implementation detail that the implementor of an Ingestion has to account for this.
                    self.sink.set_state(SinkState::Active);
                }
            }
            Err(e) => {
                self.sink.set_state(SinkState::Error(format!("{:?}", e)));
                return Err(());
            }
        }

        Ok(())
    }

    // Loads the samples into the sink.
    fn load_into_sink(&self, offset: usize, amount: usize) -> Result<LoadResult, Box<dyn Error>> {
        let old_offset = self.offset.load();
        let mut seeked_offset = offset;

        if old_offset != offset {
            seeked_offset = self.seek(offset)?;
        }

        let result = self.decode_until_filled(amount)?;

        // Skip the seek difference, to avoid artifacts.
        let start = offset.saturating_sub(seeked_offset);
        let samples = &result.samples[start..];

        self.sink.write(offset, samples);
        self.offset.store(old_offset + result.samples.len());

        Ok(result)
    }

    // Attempts to seek to the given offset.
    #[allow(dead_code)]
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
                Err(err) => return Err(err.into()),
            }
        }

        Ok(LoadResult {
            samples,
            end_reached,
        })
    }
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

#[cfg(test)]
mod tests {

    use crate::implementors::{tests::test_file, LoadableNetworkStream};

    use super::*;

    #[tokio::test]
    async fn test_symphonia_ingestion_with_local_file() {
        let file = test_file("submersion.flac").await;
        let config = Config::default();
        let ingestion = SymphoniaIngestion::new(config.clone());
        let sink = ingestion.ingest(file).await.unwrap();

        // Load all samples.
        ingestion
            .request_load(sink.id, 0, config.seconds_to_samples(60. * 5.))
            .await;

        // If successful, the sink should be in the `Sealed` state.
        assert_eq!(sink.state(), SinkState::Sealed);
    }

    #[tokio::test]
    async fn test_symphonia_ingestion_with_remote_stream() {
        let stream = LoadableNetworkStream::new(
            "https://cdn.freesound.org/previews/618/618063_1956076-lq.mp3
        ",
        )
        .await
        .unwrap();

        let config = Config::default();
        let ingestion = SymphoniaIngestion::new(config.clone());
        let sink = ingestion.ingest(stream).await.unwrap();

        for i in 0..50 {
            ingestion.request_load(sink.id, i * 8192, 8192).await;
        }

        // If successful, the sink should be in the `Sealed` state.
        assert_eq!(sink.state(), SinkState::Sealed);
    }
}
