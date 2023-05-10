use std::{fmt::Display, io::Read};

use hyper::StatusCode;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use regex::Regex;
use serde::Deserialize;

use crate::{
    audio::SAMPLES_PER_SEC,
    http::stream::ByteRangeStream,
    ingest::{
        ffmpeg,
        loading::{LoadResult, Loader, ProbeResult},
        SinkLength,
    },
};

use super::InputError;
use reqwest::blocking::Client;

lazy_static! {
    static ref REGEX: Regex = Regex::new(
        r"^(?:https?://)?wavedistrict\.com/@(?P<username>[a-z0-9-]+)/tracks/(?P<slug>[a-z0-9-]+)/?$"
    )
    .unwrap();
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Source {
    ext: String,
    name: String,
    streamable: bool,
    quality_rating: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct Media {
    key: String,
    sources: Vec<Source>,
    metadata: AudioMetadata,
}

#[derive(Debug, Clone, Deserialize)]
struct AudioMetadata {
    duration: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Track {
    id: u32,
    title: String,
    audio: Media,
}

#[derive(Debug)]
pub struct TrackLoader {
    track: Track,
    stream_url: String,
    stream: Mutex<ByteRangeStream>,
}

impl Track {
    pub fn from_url(url: &str) -> Result<Self, InputError> {
        let (username, slug) = REGEX
            .captures(url)
            .and_then(|c| c.name("username").zip(c.name("slug")))
            .map(|(u, s)| (u.as_str(), s.as_str()))
            .ok_or(InputError::NoMatch)?;

        let api_url = format!(
            "https://api.wavedistrict.com/users/{}/tracks/{}",
            username, slug
        );

        let track: Track = Client::new()
            .get(api_url)
            .send()
            .map_err(|err| match err.status() {
                Some(StatusCode::NOT_FOUND) => InputError::NotFound,
                _ => InputError::NetworkFailed,
            })
            .and_then(|r| r.json().map_err(|x| InputError::Other(Box::new(x))))?;

        Ok(track)
    }

    pub fn fingerprint(&self) -> String {
        self.audio
            .best_source_url()
            .unwrap_or_else(|| format!("wd:{}", self.id))
    }

    pub fn loader(&self) -> Result<Box<dyn Loader>, InputError> {
        let audio_url = self.audio.best_source_url().ok_or(InputError::Invalid)?;
        let stream = ByteRangeStream::try_new(audio_url.clone()).ok_or(InputError::Unknown)?;

        Ok(Box::new(TrackLoader {
            track: self.clone(),
            stream: stream.into(),
            stream_url: audio_url,
        }))
    }

    pub fn duration(&self) -> f32 {
        self.audio.metadata.duration
    }
}

impl Media {
    fn best_source_url(&self) -> Option<String> {
        let best_source = self
            .sources
            .iter()
            .max_by(|a, b| a.quality_rating.cmp(&b.quality_rating));

        best_source.map(|source| {
            format!(
                "https://media.wavedistrict.com/audio/{}.{}?source={}",
                self.key, source.ext, source.name
            )
        })
    }
}

impl Loader for TrackLoader {
    fn load(&mut self, amount: usize) -> crate::ingest::loading::LoadResult {
        let mut buf = vec![0; amount];

        let bytes_read = self.stream.lock().read(&mut buf).unwrap_or_default();
        let bytes: Vec<_> = buf[..bytes_read].to_vec();

        if bytes_read > 0 {
            LoadResult::Data(bytes)
        } else {
            LoadResult::Empty
        }
    }

    fn probe(&self) -> Option<crate::ingest::loading::ProbeResult> {
        ffmpeg::probe(&self.stream_url).map(|probe| {
            let length_in_samples = (probe.duration * SAMPLES_PER_SEC as f32).floor() as usize;

            ProbeResult {
                length: SinkLength::Exact(length_in_samples),
            }
        })
    }
}

impl Display for Track {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.title)
    }
}
