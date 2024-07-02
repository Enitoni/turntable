use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
    http::{header, request::Parts, StatusCode},
};
use turntable_collab::{SessionData, UserData};

use crate::ServerContext;

/// Wraps [SessionData] so [FromRequestParts] can be implemented for it
pub struct Session(SessionData);

impl Session {
    /// Returns the user of the session
    pub fn user(&self) -> UserData {
        self.0.user.clone()
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for Session
where
    ServerContext: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let context = ServerContext::from_ref(state);

        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|x| x.to_str().ok())
            .ok_or((StatusCode::UNAUTHORIZED, "Missing authorization"))?;

        let parts: Vec<_> = token.split_ascii_whitespace().collect();

        if parts.first() != Some(&"Bearer") {
            return Err((StatusCode::BAD_REQUEST, "Authorization must be Bearer"));
        }

        let token = parts.last().cloned().unwrap_or_default();

        let session = context
            .auth
            .session(token)
            .await
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Session does not exist"))?;

        Ok(Self(session))
    }
}
