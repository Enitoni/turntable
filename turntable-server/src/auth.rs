use std::ops::Deref;

use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
    http::{header, request::Parts, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json,
};
use turntable_collab::{Credentials, NewPlainUser, SessionData, UserData};

use crate::{
    errors::ServerResult,
    schemas::{LoginSchema, RegisterSchema, ValidatedJson},
    serialized::{LoginResult, ToSerialized, User},
    Router, ServerContext,
};

/// Wraps [SessionData] so [FromRequestParts] can be implemented for it
pub struct Session(SessionData);

impl Session {
    /// Returns the user of the session
    pub fn user(&self) -> UserData {
        self.user.clone()
    }
}

impl Deref for Session {
    type Target = SessionData;

    fn deref(&self) -> &Self::Target {
        &self.0
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

#[utoipa::path(
    get, 
    path = "/v1/auth/user",
    tag = "auth",
    security(
        ("BearerAuth" = [])
    ),
    responses(
        (status = 200, body = User)
    )
)]
async fn user(session: Session) -> impl IntoResponse {
    Json(session.user().to_serialized())
}

#[utoipa::path(
    post,
    path = "/v1/auth/register",
    tag = "auth",
    request_body = RegisterSchema,
    responses(
        (status = 200, body = User)
    )
)]
async fn register(
    context: ServerContext,
    ValidatedJson(body): ValidatedJson<RegisterSchema>,
) -> ServerResult<Json<User>> {
    let new_user = context
        .collab
        .auth
        .register_superuser(NewPlainUser {
            username: body.username,
            password: body.password,
            display_name: body.display_name,
        })
        .await?;

    Ok(Json(new_user.to_serialized()))
}

#[utoipa::path(
    post,
    path = "/v1/auth/login",
    tag = "auth",
    request_body = LoginSchema,
    responses(
        (status = 200, body = LoginResult)
    )
)]
async fn login(
    context: ServerContext,
    ValidatedJson(body): ValidatedJson<LoginSchema>,
) -> ServerResult<Json<LoginResult>> {
    let session = context
        .collab
        .auth
        .login(Credentials {
            username: body.username,
            password: body.password,
        })
        .await?;

    Ok(Json(session.to_serialized()))
}

#[utoipa::path(
    post,
    path = "/v1/auth/logout",
    tag = "auth",
    security(
        ("BearerAuth" = [])
    ),
    responses(
        (status = 200)
    )
)]
async fn logout(context: ServerContext, session: Session) -> ServerResult<()> {
    context.collab.auth.logout(&session.token).await?;
    Ok(())
}

pub fn router() -> Router {
    Router::new()
        .route("/user", get(user))
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/logout", post(logout))
}
