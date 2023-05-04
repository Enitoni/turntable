use axum::response::IntoResponse;
use hyper::StatusCode;
use thiserror::Error;

#[derive(Debug, Clone)]
pub enum Input {
    WaveDistrict(wavedistrict::Track),
    YouTube(YouTubeVideo),
    Url(Url),
}

#[derive(Debug, Error)]
pub enum InputError {
    #[error("Input type is supported but resource was not found")]
    NotFound,

    #[error("Input did not match")]
    NoMatch,

    #[error("Unsupported input type")]
    UnsupportedType,

    #[error("Failed to fetch resource")]
    NetworkFailed,

    #[error("Resource is invalid")]
    Invalid,

    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send>),

    #[error("An unknown error occurred")]
    Unknown,
}

impl Input {
    /// Returns the fingerprint used to check
    /// if this is already in cache
    pub fn fingerprint(&self) -> String {
        match self {
            Input::WaveDistrict(t) => t.fingerprint(),
            Input::YouTube(v) => v.fingerprint(),
            Input::Url(x) => x.fingerprint(),
        }
    }

    pub fn parse(str: &str) -> Result<Self, InputError> {
        let predicates = [
            |url| YouTubeVideo::from_url(url).map(Self::YouTube),
            |url| wavedistrict::Track::from_url(url).map(Self::WaveDistrict),
        ];

        predicates
            .into_iter()
            .map(|f| f(str))
            .find_map(|r| match r {
                Err(InputError::NoMatch) => None,
                x => Some(x),
            })
            .unwrap_or(Err(InputError::NoMatch))
    }

    pub fn loader(&self) -> Result<Box<dyn Loader>, InputError> {
        match self {
            Input::YouTube(video) => video.loader(),
            Input::WaveDistrict(track) => track.loader(),
            Input::Url(_) => todo!(),
        }
    }
}

impl Display for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            Input::WaveDistrict(x) => x.fmt(f),
            Input::YouTube(x) => x.fmt(f),
            Input::Url(x) => x.fmt(f),
        }
    }
}

impl From<&Input> for String {
    fn from(x: &Input) -> Self {
        x.to_string()
    }
}

impl IntoResponse for InputError {
    fn into_response(self) -> axum::response::Response {
        let status = match self {
            InputError::NotFound => StatusCode::NOT_FOUND,
            InputError::NoMatch => StatusCode::BAD_REQUEST,
            InputError::UnsupportedType => StatusCode::BAD_REQUEST,
            InputError::Invalid => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, self.to_string()).into_response()
    }
}

use std::fmt::Display;

pub use url::Url;
mod url {
    use std::fmt::Display;

    #[derive(Debug, Clone)]
    pub struct Url {
        url: String,
    }

    impl Url {
        pub fn from_url(url: &str) -> Self {
            Self {
                url: url.to_string(),
            }
        }

        pub fn fingerprint(&self) -> String {
            self.url.to_owned()
        }
    }

    impl Display for Url {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.url)
        }
    }
}

pub use youtube::YouTubeVideo;

use super::loading::Loader;
mod youtube {
    use std::{
        fmt::Display,
        io::Read,
        process::{Command, Stdio},
    };

    use log::error;
    use parking_lot::Mutex;
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

    /// Parsed from youtube-dl
    #[derive(Debug, Clone)]
    pub struct YouTubeVideo {
        id: String,
        title: String,
        duration: f32,
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

        pub fn duration(&self) -> f32 {
            self.duration
        }

        pub fn from_url(url: &str) -> Result<Self, InputError> {
            if !Self::is_valid_url(url) {
                return Err(InputError::NoMatch);
            }

            parse_from_url(url).ok_or(InputError::NotFound)
        }

        pub fn loader(&self) -> Result<Box<dyn Loader>, InputError> {
            let stream = ByteRangeStream::try_new(self.audio_stream_url.clone())
                .ok_or(InputError::Unknown)?
                .into();

            let video = self.clone().into();
            Ok(Box::new(YouTubeVideoLoader { video, stream }))
        }

        /// Returns true if this is a valid YouTube video url
        fn is_valid_url(url: &str) -> bool {
            // Remove protocol if any
            let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);

            // Remove www if any
            let rest = rest
                .split_once("www.")
                .map(|(_, rest)| rest)
                .unwrap_or(rest);

            let mut split = rest.split('/');
            let domain = split.next();
            let path = split.next();

            domain
                .zip(path)
                .map(|(domain, path)| {
                    domain == "youtube.com" && path.starts_with("watch?v=") || domain == "youtu.be"
                })
                .unwrap_or_default()
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
                        duration: raw_video.duration,
                        audio_stream_url: format.url.to_owned(),
                    })
            })
    }

    #[cfg(test)]
    mod test {
        use super::YouTubeVideo;

        #[test]
        fn test_url() {
            assert!(YouTubeVideo::is_valid_url(
                "https://www.youtube.com/watch?v=RiZ_5jo9WBg"
            ));
            assert!(YouTubeVideo::is_valid_url(
                "https://youtube.com/watch?v=RiZ_5jo9WBg"
            ));
            assert!(YouTubeVideo::is_valid_url(
                "youtube.com/watch?v=RiZ_5jo9WBg"
            ));

            assert!(!YouTubeVideo::is_valid_url(
                "yourtube.com/watch?v=RiZ_5jo9WBg"
            ));
            assert!(!YouTubeVideo::is_valid_url("https://google.com"));
            assert!(!YouTubeVideo::is_valid_url("kpofkagt"));
        }
    }
}

mod wavedistrict {
    use std::{fmt::Display, io::Read};

    use fundsp::lazy_static::lazy_static;
    use hyper::StatusCode;
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
        ).unwrap();
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
}
