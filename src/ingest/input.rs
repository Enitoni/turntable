#[derive(Debug, Clone)]
pub enum Input {
    YouTube(YouTubeVideo),
    Url(Url),
}

impl Input {
    /// Returns the fingerprint used to check
    /// if this is already in cache
    pub fn fingerprint(&self) -> String {
        match self {
            Input::YouTube(v) => v.fingerprint(),
            Input::Url(x) => x.fingerprint(),
        }
    }

    pub fn parse(str: &str) -> Option<Self> {
        let predicates = [
            |url| YouTubeVideo::from_url(url).map(Self::YouTube),
            |url| Self::Url(Url::from_url(url)).into(),
        ];

        predicates.into_iter().find_map(|f| f(str))
    }

    pub fn loader(&self) -> Box<dyn Loader> {
        match self {
            Input::YouTube(video) => Box::new(video.loader()),
            Input::Url(_) => todo!(),
        }
    }
}

impl Display for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            Input::YouTube(x) => std::fmt::Display::fmt(&x, f),
            Input::Url(x) => std::fmt::Display::fmt(&x, f),
        }
    }
}

impl From<&Input> for String {
    fn from(x: &Input) -> Self {
        x.to_string()
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
        io::{self, Read},
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

        pub fn from_url(url: &str) -> Option<Self> {
            if !Self::is_valid_url(url) {
                return None;
            }

            parse_from_url(url)
        }

        pub fn loader(&self) -> YouTubeVideoLoader {
            YouTubeVideoLoader::new(self.clone())
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

    impl YouTubeVideoLoader {
        pub fn new(video: YouTubeVideo) -> Self {
            let stream = ByteRangeStream::try_new(video.audio_stream_url.clone())
                .expect("HANDLE THIS IN THE FUTURE")
                .into();

            let video = video.into();
            Self { video, stream }
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
