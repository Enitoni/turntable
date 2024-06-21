use async_trait::async_trait;
use thiserror::Error;
use turntable_core::BoxedLoadable;
use youtube::YouTubeVideoInput;

mod youtube;

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

    #[error("{0}")]
    Other(String),

    #[error("An unknown error occurred")]
    Unknown,
}

/// Represents any resource that can be used as an input for turntable
pub enum Input {
    YouTube(youtube::YouTubeVideoInput),
}

impl Input {
    pub async fn query(input: &str) -> Result<Vec<Self>, InputError> {
        if YouTubeVideoInput::test(input) {
            let results = YouTubeVideoInput::fetch(input).await?;
            return Ok(results.into_iter().map(Input::YouTube).collect());
        }

        Err(InputError::NoMatch)
    }

    pub async fn loadable(&self) -> Result<BoxedLoadable, InputError> {
        match self {
            Input::YouTube(input) => input.loadable().await,
        }
    }

    pub fn length(&self) -> Option<f32> {
        match self {
            Input::YouTube(input) => input.length(),
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
    async fn loadable(&self) -> Result<BoxedLoadable, InputError>;
}
