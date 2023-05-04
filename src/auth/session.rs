use anyhow::Result;
use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
};
use hyper::{header, http::request::Parts, StatusCode};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::{Deserialize, Serialize};
use serde_json::json;
use surrealdb::sql::Thing;
use tokio::task::spawn_blocking;

use crate::{
    db::{Database, Error, Record},
    util::ApiError,
    VinylContext,
};

use super::user::User;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Session {
    pub id: Thing,
    pub user: User,
}

impl Session {
    pub async fn create(db: &Database, user: &User) -> Result<Self, ApiError> {
        let user = user.id.clone().ok_or(ApiError::Unknown)?;

        let token: String = spawn_blocking(|| {
            let mut rng = thread_rng();

            std::iter::repeat(())
                .map(|_| rng.sample(Alphanumeric) as char)
                .take(32)
                .collect()
        })
        .await
        .map_err(|e| ApiError::Other(e.into()))?;

        #[derive(Serialize)]
        struct NewSession {
            id: String,
            user: Thing,
        }

        let session: Record = db
            .create("session")
            .content(NewSession { id: token, user })
            .await?;

        let session = Self::get(db, &session.id().to_string()).await?;

        Ok(session)
    }

    pub async fn get(db: &Database, token: &str) -> Result<Self, ApiError> {
        db.query("SELECT *, user.* FROM type::thing($tb, $id)")
            .bind(("tb", "session"))
            .bind(("id", token))
            .await?
            .take::<Option<Self>>(0)?
            .ok_or(ApiError::NotFound("session"))
    }

    pub fn token(&self) -> String {
        self.id.id.to_string()
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for Session
where
    VinylContext: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let context = VinylContext::from_ref(state);

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
        let session = Self::get(&context.db, token)
            .await
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Session does not exist"))?;

        Ok(session)
    }
}
