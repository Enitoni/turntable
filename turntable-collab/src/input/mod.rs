use async_trait::async_trait;
use thiserror::Error;
use turntable_core::BoxedLoadable;
use wavedistrict::WaveDistrictTrackInput;
use youtube::YouTubeVideoInput;

mod file;
mod wavedistrict;
mod youtube;

#[derive(Debug, Error)]
pub enum InputError {
    #[error("Input did not match")]
    NoMatch,

    #[error("Input is invalid: {0}")]
    Invalid(String),

    #[error("Input type is supported but resource was not found")]
    NotFound,

    #[error("Resource was found but is unavailable")]
    Unavailable,

    #[error("Unsupported input type")]
    UnsupportedType,

    #[error("Failed to fetch resource: {0}")]
    FetchError(String),

    #[error("Failed to parse resource: {0}")]
    ParseError(String),

    #[error("{0}")]
    Other(String),
}

/// Represents metadata of the input
#[derive(Debug, Clone)]
pub struct Metadata {
    pub title: String,
    pub artist: Option<String>,

    pub canonical: String,
    pub source: String,

    pub duration: f32,
    pub artwork: Option<String>,
}

/// Represents any resource that can be used as an input for turntable
#[derive(Debug)]
pub enum Input {
    WaveDistrict(wavedistrict::WaveDistrictTrackInput),
    YouTube(youtube::YouTubeVideoInput),
    File(file::FileInput),
}

impl Input {
    pub async fn query(input: &str) -> Result<Vec<Self>, InputError> {
        if YouTubeVideoInput::test(input) {
            let results = YouTubeVideoInput::fetch(input).await?;
            return Ok(results.into_iter().map(Input::YouTube).collect());
        }

        if WaveDistrictTrackInput::test(input) {
            let results = WaveDistrictTrackInput::fetch(input).await?;
            return Ok(results.into_iter().map(Input::WaveDistrict).collect());
        }

        if file::FileInput::test(input) {
            let results = file::FileInput::fetch(input).await?;
            return Ok(results.into_iter().map(Input::File).collect());
        }

        Err(InputError::NoMatch)
    }

    pub fn loadable(&self) -> BoxedLoadable {
        match self {
            Input::WaveDistrict(input) => input.loadable(),
            Input::YouTube(input) => input.loadable(),
            Input::File(input) => input.loadable(),
        }
    }

    pub fn length(&self) -> Option<f32> {
        match self {
            Input::WaveDistrict(input) => input.length(),
            Input::YouTube(input) => input.length(),
            Input::File(input) => input.length(),
        }
    }

    pub fn metadata(&self) -> Metadata {
        match self {
            Input::WaveDistrict(input) => input.metadata(),
            Input::YouTube(input) => input.metadata(),
            Input::File(input) => input.metadata(),
        }
    }
}

/// Represents a type that can be used as an input to turntable
#[async_trait]
pub trait Inputable {
    /// Returns true if the given query matches the pattern of this inputable
    fn test(query: &str) -> bool;

    /// Attempts to fetch the resource from the given query.
    /// This can return multiple results if the query is a playlist.
    async fn fetch(query: &str) -> Result<Vec<Self>, InputError>
    where
        Self: Sized;

    /// Returns the length of the resource in seconds if known.
    fn length(&self) -> Option<f32>;

    /// "Activates" the resource, returning a loadable that can be used to play it.
    fn loadable(&self) -> BoxedLoadable;

    /// Returns the metadata of the input
    fn metadata(&self) -> Metadata;
}
