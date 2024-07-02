use aide::{
    axum::{routing::get_with, IntoApiResponse},
    transform::TransformOperation,
    OperationInput,
};
use axum::{
    async_trait, debug_handler,
    extract::{FromRef, FromRequestParts},
    http::{header, request::Parts, StatusCode},
    Json,
};
use turntable_collab::{SessionData, UserData};

use crate::{serialized::ToSerialized, Router, ServerContext};

/// Wraps [SessionData] so [FromRequestParts] can be implemented for it
pub struct Session(SessionData);

impl Session {
    /// Returns the user of the session
    pub fn user(&self) -> UserData {
        self.0.user.clone()
    }
}

#[async_trait]
impl FromRequestParts<ServerContext> for Session {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ServerContext,
    ) -> Result<Self, Self::Rejection> {
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
            .collab
            .auth
            .session(token)
            .await
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Session does not exist"))?;

        Ok(Self(session))
    }
}

/// A helper function to add auth information to routes
fn with_auth(transform: TransformOperation) -> TransformOperation {
    transform
        .security_requirement("http")
        .response_with::<401, String, _>(|r| {
            r.description("Request refused because of missing authorization")
                .example("Missing authorization")
        })
        .response_with::<400, String, _>(|r| {
            r.description("Request refused because Authorization header is incorrect")
                .example("Authorization must be Bearer")
        })
}

impl OperationInput for Session {}

#[debug_handler(state = ServerContext)]
async fn user(session: Session) -> impl IntoApiResponse {
    Json(session.user().to_serialized())
}

pub fn router() -> Router {
    Router::new().api_route(
        "/user",
        get_with(user, |op| {
            with_auth(op.description("Gets the user associated with the supplied session"))
        }),
    )
}
