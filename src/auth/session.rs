use anyhow::Result;
use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
};
use hyper::{header, http::request::Parts, StatusCode};
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use serde_json::json;
use surrealdb::sql::Thing;

use crate::{
    db::{Database, Error},
    VinylContext,
};

use super::user::User;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Session {
    pub id: Thing,
    pub user: User,
}

impl Session {
    pub async fn create(db: &Database, user: &User) -> Result<Self> {
        let mut rng = thread_rng();

        let user = user.id.clone().ok_or(Error::Unknown)?;

        let token: String = std::iter::repeat(())
            .map(|_| rng.gen::<u8>())
            .map(char::from)
            .take(32)
            .collect();

        let session: Self = db
            .create("session")
            .content(json!({
                "id": token,
                "user": user,
            }))
            .await?;

        Ok(session)
    }

    pub async fn get(db: &Database, token: &str) -> Result<Self> {
        db.query("SELECT * FROM session:$token")
            .bind(("token", token))
            .await?
            .take::<Option<Self>>(0)?
            .ok_or(Error::NotFound("session").into())
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
