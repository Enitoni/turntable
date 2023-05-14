use std::{
    fmt::Display,
    io::Read,
    process::{Command, Stdio},
};

use lazy_static::lazy_static;
use log::error;
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
    track::Metadata,
};

use super::InputError;

lazy_static! {
    static ref REGEX: Regex =
        Regex::new(r"^(?:https?://)?(?:.+\.)?youtube\.com/(?:watch\?v=|v/)[A-Za-z\d_-]+").unwrap();
}

/// Parsed from youtube-dl
#[derive(Debug, Clone)]
pub struct YouTubeVideo {
    id: String,
    title: String,
    duration: f32,
    thumbnail: String,
    channel: String,
    audio_stream_url: String,
}

#[derive(Debug, Deserialize)]
struct RawFormat {
    format_id: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct RawYouTubeVideo {
    id: String,
    title: String,
    channel: String,
    thumbnail: String,
    format_id: String,
    formats: Vec<RawFormat>,
    duration: f32,
}

#[derive(Debug)]
pub struct YouTubeVideoLoader {
    video: Mutex<YouTubeVideo>,
    stream: Mutex<ByteRangeStream>,
}

impl YouTubeVideo {
    pub fn fingerprint(&self) -> String {
        self.title.to_owned()
    }

    pub fn metadata(&self) -> Metadata {
        Metadata {
            title: self.title.clone(),
            artist: self.channel.clone(),
            canonical: format!("https://youtube.com/v/{}", self.id),
            source: "youtube".to_string(),
            duration: self.duration,
            artwork: Some(self.thumbnail.clone()),
        }
    }

    pub fn from_url(url: &str) -> Result<Self, InputError> {
        let url = REGEX
            .find(url)
            .map(|m| m.as_str())
            .ok_or(InputError::NoMatch)?;

        parse_from_url(url).ok_or(InputError::NotFound)
    }

    pub fn loader(&self) -> Result<Box<dyn Loader>, InputError> {
        let stream = ByteRangeStream::try_new(self.audio_stream_url.clone())
            .ok_or(InputError::Unknown)?
            .into();

        let video = self.clone().into();
        Ok(Box::new(YouTubeVideoLoader { video, stream }))
    }
}

impl Loader for YouTubeVideoLoader {
    fn load(&mut self, amount: usize) -> LoadResult {
        let mut buf = vec![0; amount];

        let bytes_read = self.stream.lock().read(&mut buf).unwrap_or_default();
        let bytes: Vec<_> = buf[..bytes_read].to_vec();

        if bytes_read > 0 {
            LoadResult::Data(bytes)
        } else {
            LoadResult::Empty
        }
    }

    fn probe(&self) -> Option<ProbeResult> {
        ffmpeg::probe(&self.video.lock().audio_stream_url).map(|probe| {
            let length_in_samples = (probe.duration * SAMPLES_PER_SEC as f32).floor() as usize;

            ProbeResult {
                length: SinkLength::Exact(length_in_samples),
            }
        })
    }
}

impl Display for YouTubeVideo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} by {}", self.title, self.channel)
    }
}

/// Tries to fetch the video via youtube-dl, returning None if important
/// fields are missing or the fetch failed.
pub fn parse_from_url(url: &str) -> Option<YouTubeVideo> {
    let mut child = Command::new("yt-dlp")
        .arg("-f")
        .arg("bestaudio/best")
        .arg("-j")
        .arg("--")
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("yt-dlp failed to spawn");

    let stdout = child.stdout.take().unwrap();
    let video: Result<RawYouTubeVideo, serde_json::Error> = serde_json::from_reader(stdout);

    child.wait().expect("yt-dlp exits");

    video
        .map_err(|err| {
            error!("Failed to fetch YouTube video: {}", err.to_string());
        })
        .ok()
        .and_then(|raw_video| {
            raw_video
                .formats
                .iter()
                .find(|f| f.format_id == raw_video.format_id)
                .map(|format| YouTubeVideo {
                    id: raw_video.id,
                    title: raw_video.title,
                    channel: raw_video.channel,
                    thumbnail: raw_video.thumbnail,
                    duration: raw_video.duration,
                    audio_stream_url: format.url.to_owned(),
                })
        })
}
