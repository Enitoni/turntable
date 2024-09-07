#![allow(dead_code)]
// There are many fields we wanna use here, but we're not using them yet. The warnings are annoying, so they're disabled for now.

use async_trait::async_trait;
use parking_lot::Mutex;
use serde::Deserialize;
use std::env;
use std::io::SeekFrom;
use std::process::Stdio;
use std::sync::Arc;
use std::{error::Error, fmt::Debug};
use tokio::{io::AsyncReadExt, process::Command};
use turntable_core::{BoxedLoadable, Loadable, LoaderLength, ReadResult};
use turntable_impls::LoadableNetworkStream;
use url::Url;

use crate::{util::URL_SCHEME_REGEX, Metadata};

use super::{InputError, Inputable};

const YT_UNAVAILABLE: &str = "Video unavailable. This video is not available";
const YT_TOO_MANY_REQUESTS: &str = "Too Many Requests";
const YT_NOT_FOUND: &str = "Video unavailable";
const YT_ID_ERROR: &str = "Incomplete YouTube ID";

/// A YouTube video that can be played by turntable.
#[derive(Clone)]
pub struct YouTubeVideoInput {
    id: String,
    title: String,
    duration: f32,
    thumbnail: String,
    channel: String,
}

#[derive(Debug, Deserialize)]
struct Format {
    url: String,
    format_id: String,
    format: String,
    container: Option<String>,
    quality: Option<f32>,
    acodec: Option<String>,
    audio_ext: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FlatYouTubeVideo {
    id: String,
    title: String,
    channel: String,
    thumbnails: Vec<Thumbnail>,
    duration: f32,
}

#[derive(Debug, Deserialize)]
struct Thumbnail {
    url: String,
    width: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct PlayableYouTubeVideo {
    format_id: String,
    formats: Vec<Format>,
}

#[derive(Debug, Deserialize)]
struct DeletedYouTubeVideo {
    id: String,
    duration: (),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum YouTubeVideo {
    Flat(FlatYouTubeVideo),
    Deleted(DeletedYouTubeVideo),
}

#[derive(Debug, Deserialize)]
struct YouTubePlaylist {
    entries: Vec<YouTubeVideo>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum YouTubeResource {
    Video(FlatYouTubeVideo),
    Playlist(YouTubePlaylist),
}

/// Wraps [LoadableNetworkStream] because the stream url has to be retrieved on demand
pub struct LoadableYouTubeVideo {
    id: String,
    stream: Mutex<Option<Arc<LoadableNetworkStream>>>,
}

#[async_trait]
impl Inputable for YouTubeVideoInput {
    fn test(query: &str) -> bool {
        let query = URL_SCHEME_REGEX.replace(query, "https://");
        let url = Url::parse(&query);

        match url {
            Ok(url) => {
                // Test youtube.com
                if url
                    .host_str()
                    .filter(|s| s.ends_with("youtube.com"))
                    .is_some()
                {
                    // Test /watch?v=...
                    if url.path().starts_with("/watch")
                        && url
                            .query_pairs()
                            .into_iter()
                            .any(|(k, v)| k == "v" && !v.is_empty())
                    {
                        return true;
                    }

                    // Test /v/...
                    if url.path().starts_with("/v/") {
                        return true;
                    }

                    // Test playlists
                    if url.path() == "/playlist"
                        && url
                            .query_pairs()
                            .into_iter()
                            .any(|(k, v)| k == "list" && !v.is_empty())
                    {
                        return true;
                    }
                }

                // Test youtu.be/...
                if url.host_str() == Some("youtu.be") && !url.path().is_empty() {
                    return true;
                }

                false
            }
            Err(_) => false,
        }
    }

    async fn fetch(query: &str) -> Result<Vec<Self>, InputError>
    where
        Self: Sized,
    {
        let resource = YouTubeResource::fetch(query).await?;

        match resource {
            YouTubeResource::Video(video) => Ok(vec![video.into()]),
            YouTubeResource::Playlist(playlist) => Ok(playlist
                .entries
                .into_iter()
                .filter_map(|v| match v {
                    YouTubeVideo::Flat(v) => Some(v),
                    YouTubeVideo::Deleted(_) => None,
                })
                .map(Into::into)
                .collect()),
        }
    }

    fn length(&self) -> Option<f32> {
        Some(self.duration)
    }

    fn loadable(&self) -> BoxedLoadable {
        LoadableYouTubeVideo {
            id: self.id.clone(),
            stream: Default::default(),
        }
        .boxed()
    }

    fn metadata(&self) -> Metadata {
        Metadata {
            title: self.title.clone(),
            artist: Some(self.channel.clone()),
            duration: self.duration,
            artwork: Some(self.thumbnail.clone()),
            canonical: format!("https://youtube.com/v/{}", self.id),
            source: "youtube".to_string(),
        }
    }
}

impl YouTubeResource {
    /// Attempts to fetch a video or several videos from the given URL using yt-dlp.
    pub async fn fetch(url: &str) -> Result<Self, InputError> {
        let mut command = Command::new("yt-dlp");

        // Workaround for YouTube blocking anonymous VPS
        if let Ok(cookie_path) = env::var("YOUTUBE_COOKIE_FILE") {
            command.args(["--cookies", &cookie_path]);
        }

        command
            // Don't try to get a stream url for playlists.
            .arg("--flat-playlist")
            // Or videos.
            .arg("--skip-download")
            // Get a JSON output, in a single line.
            .arg("-J")
            .args(["--", url])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|e| InputError::Other(e.to_string()))?;

        let mut output = String::new();
        let mut error_output = String::new();

        child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut output)
            .await
            .map_err(|e| InputError::Other(e.to_string()))?;

        child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut error_output)
            .await
            .ok();

        let exit = child
            .wait()
            .await
            .map_err(|e| InputError::Other(e.to_string()))?;

        if !exit.success() {
            return Err(handle_error(error_output));
        }

        let entry: YouTubeResource =
            serde_json::from_str(&output).map_err(|e| InputError::ParseError(e.to_string()))?;

        Ok(entry)
    }
}

impl LoadableYouTubeVideo {
    async fn setup(&self) -> Result<(), Box<dyn Error>> {
        let url = format!("https://youtube.com/watch?v={}", self.id);

        let mut command = Command::new("yt-dlp");

        // Workaround for YouTube blocking anonymous VPS
        if let Ok(cookie_path) = env::var("YOUTUBE_COOKIE_FILE") {
            command.args(["--cookies", &cookie_path]);
        }

        command
            .arg("-f")
            .arg("bestaudio[ext=mp3]/best")
            .arg("-j")
            .arg("--")
            .arg(url)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn()?;

        let mut output = String::new();
        let mut error_output = String::new();

        child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut output)
            .await?;

        child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut error_output)
            .await
            .ok();

        let exit = child.wait().await?;

        if !exit.success() {
            return Err(Box::new(handle_error(error_output)));
        }

        let entry: PlayableYouTubeVideo =
            serde_json::from_str(&output).map_err(|e| InputError::ParseError(e.to_string()))?;

        let stream_url = entry
            .formats
            .iter()
            .find(|f| f.format_id == entry.format_id)
            .map(|f| f.url.to_owned())
            .ok_or(InputError::Other("No supported format found".to_string()))?;

        *self.stream.lock() = Some(Arc::new(LoadableNetworkStream::new(stream_url)));
        Ok(())
    }

    fn stream(&self) -> Arc<LoadableNetworkStream> {
        self.stream
            .lock()
            .as_ref()
            .expect("stream exists")
            .to_owned()
    }
}

#[async_trait]
impl Loadable for LoadableYouTubeVideo {
    async fn activate(&self) -> Result<(), Box<dyn Error>> {
        dbg!("hi");

        self.setup().await?;
        self.stream().activate().await?;

        Ok(())
    }

    async fn read(&self, buf: &mut [u8]) -> Result<ReadResult, Box<dyn Error>> {
        self.stream().read(buf).await
    }

    async fn length(&self) -> Option<LoaderLength> {
        self.stream().length().await
    }

    async fn seek(&self, seek: SeekFrom) -> Result<usize, Box<dyn Error>> {
        self.stream().seek(seek).await
    }
}

impl From<FlatYouTubeVideo> for YouTubeVideoInput {
    fn from(video: FlatYouTubeVideo) -> Self {
        YouTubeVideoInput {
            id: video.id,
            title: video.title,
            duration: video.duration,
            channel: video.channel,
            thumbnail: determine_thumbnail(video.thumbnails),
        }
    }
}

impl Debug for YouTubeVideoInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "YouTube: {}", &self.id)
    }
}

fn determine_thumbnail(mut thumbnails: Vec<Thumbnail>) -> String {
    // Sort to get the largest at end
    thumbnails.sort_by(|a, b| a.width.cmp(&b.width));

    thumbnails
        .pop()
        .map(|t| t.url.replace("hqdefault", "maxresdefault"))
        .unwrap_or_default()
}

fn handle_error(error_output: String) -> InputError {
    if error_output.contains(YT_UNAVAILABLE) {
        return InputError::Unavailable;
    }

    if error_output.contains(YT_NOT_FOUND) {
        return InputError::NotFound;
    }

    if error_output.contains(YT_ID_ERROR) {
        return InputError::Invalid("Invalid Video ID".to_string());
    }

    if error_output.contains(YT_TOO_MANY_REQUESTS) {
        return InputError::FetchError("Too Many Requests".to_string());
    }

    InputError::Other(error_output)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_url_testing() {
        assert!(YouTubeVideoInput::test(
            "https://www.youtube.com/watch?v=JwRWf3ho4B8&list=PL23A657E4BD523733&index=45"
        ));
        assert!(YouTubeVideoInput::test(
            "www.youtube.com/watch?v=z09GolEktUw&feature=youtu.be"
        ));
        assert!(YouTubeVideoInput::test(
            "https://music.youtube.com/watch?v=-t-75CCdM2o"
        ));
        assert!(YouTubeVideoInput::test(
            "https://music.youtube.com/playlist?list=OLAK5uy_kKEZSgdsNQxjhnQNwMy63GMNV_ZoTqI0w"
        ));
        assert!(YouTubeVideoInput::test(
            "https://www.youtube.com/watch?v=z09GolEktUw"
        ));
        assert!(YouTubeVideoInput::test("https://youtube.com/v/z09GolEktUw"));
        assert!(YouTubeVideoInput::test("youtu.be/z09GolEktUw"));

        assert!(!YouTubeVideoInput::test("https://www.youtube.com/"));
        assert!(!YouTubeVideoInput::test("https://www.youtube.com/@Ayrun"));
        assert!(!YouTubeVideoInput::test("youtube.com/"));
    }
}
