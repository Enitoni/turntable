use async_trait::async_trait;
use reqwest::{Client, Response, StatusCode};
use serde::Deserialize;
use turntable_core::{BoxedLoadable, Loadable};
use turntable_impls::LoadableNetworkStream;
use url::Url;

use crate::{util::URL_SCHEME_REGEX, InputError, Inputable, Metadata};

const API_BASE: &str = "https://api.wavedistrict.com";
const MEDIA_BASE: &str = "https://media.wavedistrict.com";

#[derive(Debug, Clone)]
pub struct WaveDistrictTrackInput(Track);

#[derive(Debug, Clone, Deserialize)]
struct Track {
    duration: f32,
    slug: String,
    title: String,
    user: User,
    audio: MediaRef<AudioSource>,
    artwork: Option<MediaRef<ImageSource>>,
}

#[derive(Debug, Clone, Deserialize)]
struct Collection {
    id: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct User {
    username: String,
    display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MediaRef<Source> {
    key: String,
    sources: Vec<Source>,
}

#[derive(Debug, Clone, Deserialize)]
struct AudioSource {
    name: String,
    mime: String,
    ext: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ImageSource {
    width: u32,
    name: String,
    ext: String,
}

#[derive(Debug, Clone, Deserialize)]
struct List<T> {
    rows: Vec<T>,
}

#[derive(Debug, PartialEq, Eq)]
enum UrlExtraction {
    Track { user: String, slug: String },
    Collection { user: String, slug: String },
}

impl MediaRef<ImageSource> {
    fn to_path(&self) -> Option<String> {
        let mut sources: Vec<_> = self
            .sources
            .iter()
            .filter(|s| s.name != "original")
            .collect();

        // Sort to get the biggest image at the end
        sources.sort_by(|a, b| a.width.cmp(&b.width));

        let best_source = sources.pop()?;

        Some(format!(
            "{}/images/{}.{}?source={}",
            MEDIA_BASE, self.key, best_source.ext, best_source.name
        ))
    }
}

impl MediaRef<AudioSource> {
    fn to_path(&self) -> Option<String> {
        let sources: Vec<_> = self
            .sources
            .iter()
            .filter(|s| s.name != "original")
            .collect();

        let lossless = sources.iter().find(|s| s.mime == "audio/flac");
        let compressed = sources.iter().find(|s| s.mime == "audio/mpeg");

        let best_source = lossless.or(compressed)?;

        Some(format!(
            "{}/audio/{}.{}?source={}",
            MEDIA_BASE, self.key, best_source.ext, best_source.name
        ))
    }
}

impl UrlExtraction {
    async fn fetch(self) -> Result<Vec<Track>, InputError> {
        match self {
            UrlExtraction::Track { user, slug } => Ok(vec![fetch_track(&user, &slug).await?]),
            UrlExtraction::Collection { user, slug } => Ok(fetch_collection(&user, &slug).await?),
        }
    }
}

#[async_trait]
impl Inputable for WaveDistrictTrackInput {
    fn test(query: &str) -> bool {
        extract_from_url(query).is_some()
    }

    async fn fetch(query: &str) -> Result<Vec<Self>, InputError> {
        let extraction = extract_from_url(query).expect("already checked query is extractable");

        extraction
            .fetch()
            .await
            .map(|tracks| tracks.into_iter().map(Self).collect())
    }

    fn length(&self) -> Option<f32> {
        Some(self.0.duration)
    }

    fn loadable(&self) -> BoxedLoadable {
        LoadableNetworkStream::new(self.0.audio.to_path().unwrap_or_default()).boxed()
    }

    fn metadata(&self) -> Metadata {
        let artwork = self.0.artwork.clone().and_then(|m| m.to_path());
        let canonical = format!(
            "https://wavedistrict.com/@{}/tracks/{}",
            self.0.user.username, self.0.slug
        );

        Metadata {
            artist: Some(self.0.user.display_name.clone()),
            source: "wavedistrict".to_string(),
            title: self.0.title.clone(),
            duration: self.0.duration,
            canonical,
            artwork,
        }
    }
}

async fn fetch_track(user: &str, slug: &str) -> Result<Track, InputError> {
    let url = format!("{}/users/{}/tracks/{}", API_BASE, user, slug);
    let client = Client::new();

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| InputError::FetchError(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        return Err(handle_unsuccessful_request(response, status).await);
    }

    let track: Track = response.json().await.map_err(|e| {
        dbg!(&e);
        InputError::ParseError(e.to_string())
    })?;

    Ok(track)
}

async fn fetch_collection(user: &str, slug: &str) -> Result<Vec<Track>, InputError> {
    let url = format!("{}/users/{}/collections/{}", API_BASE, user, slug);
    let client = Client::new();

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| InputError::FetchError(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        return Err(handle_unsuccessful_request(response, status).await);
    }

    let collection: Collection = response
        .json()
        .await
        .map_err(|e| InputError::ParseError(e.to_string()))?;

    let url = format!(
        "{}/collections/{}/tracks?ascending=true",
        API_BASE, collection.id
    );
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| InputError::FetchError(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        return Err(handle_unsuccessful_request(response, status).await);
    }

    let result: List<Track> = response
        .json()
        .await
        .map_err(|e| InputError::ParseError(e.to_string()))?;

    Ok(result.rows)
}

fn extract_from_url(query: &str) -> Option<UrlExtraction> {
    let query = URL_SCHEME_REGEX.replace(query, "https://");
    let url = Url::parse(&query);

    match url {
        Ok(url) => {
            url.host_str().filter(|s| s.ends_with("wavedistrict.com"))?;

            let mut parts = url.path_segments()?;

            let user = parts.next()?;
            let resource = parts.next()?;
            let slug = parts.next()?;

            if !user.starts_with('@') {
                return None;
            }

            let user = user.replace('@', "");
            let slug = slug.to_string();

            match resource {
                "tracks" => Some(UrlExtraction::Track { user, slug }),
                "collections" => Some(UrlExtraction::Collection { user, slug }),
                _ => None,
            }
        }
        Err(_) => None,
    }
}

async fn handle_unsuccessful_request(response: Response, status: StatusCode) -> InputError {
    if status.as_u16() == 404 {
        return InputError::NotFound;
    }

    let result = response.text().await;

    match result {
        Ok(text) => InputError::Other(text),
        Err(e) => InputError::Other(e.to_string()),
    }
}

#[cfg(test)]
mod test {
    use crate::input::wavedistrict::{extract_from_url, UrlExtraction};

    #[test]
    fn test_url_extraction() {
        assert_eq!(
            extract_from_url("https://wavedistrict.com/@mapleleaf/tracks/traaaaaance"),
            Some(UrlExtraction::Track {
                user: "mapleleaf".to_string(),
                slug: "traaaaaance".to_string()
            })
        );
        assert_eq!(
            extract_from_url(
                "https://wavedistrict.com/@enitoni/collections/over-the-mountains/tracks"
            ),
            Some(UrlExtraction::Track {
                user: "enitoni".to_string(),
                slug: "over-the-mountains".to_string()
            })
        )
    }
}
