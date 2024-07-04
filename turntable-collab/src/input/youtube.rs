#![allow(dead_code)]
// There are many fields we wanna use here, but we're not using them yet. The warnings are annoying, so they're disabled for now.

use async_trait::async_trait;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;
use std::process::Stdio;
use tokio::{io::AsyncReadExt, process::Command};
use turntable_core::{BoxedLoadable, Loadable};
use turntable_impls::LoadableNetworkStream;

use crate::Metadata;

use super::{InputError, Inputable};

lazy_static! {
    static ref REGEX: Regex =
        Regex::new(r"^(?:https?://)?(?:[a-z]+\.)?youtube\.com/(?:(?:playlist\?list=)|(?:watch\?v=|v/))[A-Za-z\d_\-&=]+$")
            .unwrap();
}

/// A YouTube video that can be played by turntable.
#[derive(Debug, Clone)]
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
    thumbnail: String,
    duration: f32,
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

#[async_trait]
impl Inputable for YouTubeVideoInput {
    fn test(query: &str) -> bool {
        REGEX.is_match(query)
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

    async fn loadable(&self) -> Result<BoxedLoadable, InputError> {
        let url = format!("https://youtube.com/watch?v={}", self.id);

        let mut child = Command::new("yt-dlp")
            .arg("-f")
            .arg("bestaudio[ext=mp3]/best")
            .arg("-j")
            .arg("--")
            .arg(url)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| InputError::Other(e.to_string()))?;

        let mut output = String::new();

        child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut output)
            .await
            .map_err(|e| InputError::Other(e.to_string()))?;

        child
            .wait()
            .await
            .map_err(|e| InputError::Other(e.to_string()))?;

        let entry: PlayableYouTubeVideo =
            serde_json::from_str(&output).map_err(|e| InputError::ParseError(e.to_string()))?;

        let stream_url = entry
            .formats
            .iter()
            .find(|f| f.format_id == entry.format_id)
            .map(|f| f.url.to_owned())
            .ok_or(InputError::Invalid)?;

        let boxed = LoadableNetworkStream::new(stream_url)
            .await
            .map_err(|_| InputError::NetworkFailed)?
            .boxed();

        Ok(boxed)
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
        let mut child = Command::new("yt-dlp")
            // Don't try to get a stream url for playlists.
            .arg("--flat-playlist")
            // Or videos.
            .arg("--skip-download")
            // Get a JSON output, in a single line.
            .arg("-J")
            .args(["--", url])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| InputError::Other(e.to_string()))?;

        let mut output = String::new();

        child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut output)
            .await
            .map_err(|e| InputError::Other(e.to_string()))?;

        child
            .wait()
            .await
            .map_err(|e| InputError::Other(e.to_string()))?;

        let entry: YouTubeResource =
            serde_json::from_str(&output).map_err(|e| InputError::ParseError(e.to_string()))?;

        Ok(entry)
    }
}

impl From<FlatYouTubeVideo> for YouTubeVideoInput {
    fn from(video: FlatYouTubeVideo) -> Self {
        YouTubeVideoInput {
            id: video.id,
            title: video.title,
            duration: video.duration,
            thumbnail: video.thumbnail,
            channel: video.channel,
        }
    }
}
