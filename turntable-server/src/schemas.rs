use axum::{
    async_trait,
    extract::{FromRequest, Request},
    http::StatusCode,
    Json,
};

use serde::{de::DeserializeOwned, Deserialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, ToSchema, Validate, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoginSchema {
    #[validate(length(max = 128))]
    pub username: String,
    #[validate(length(max = 64))]
    pub password: String,
}

#[derive(Debug, ToSchema, Validate, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterSchema {
    #[validate(length(min = 2, max = 128))]
    pub display_name: String,
    #[validate(length(min = 2, max = 128))]
    pub username: String,
    #[validate(length(min = 8, max = 64))]
    pub password: String,
    pub invite_token: Option<String>,
}

#[derive(Debug, ToSchema, Validate, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewRoomSchema {
    #[validate(length(min = 3, max = 64))]
    pub slug: String,
    #[validate(length(min = 3, max = 64))]
    pub title: String,
    #[validate(length(max = 2048))]
    pub description: Option<String>,
}

#[derive(Debug, ToSchema, Validate, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewStreamKeySchema {
    #[validate(length(min = 2, max = 24))]
    pub source: String,
}

#[derive(Debug, ToSchema, Validate, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputSchema {
    pub query: String,
}

#[derive(Debug, ToSchema, Validate, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoinWithInviteSchema {
    pub token: String,
}

#[derive(Debug, ToSchema, Deserialize)]
#[serde(rename_all = "camelCase", tag = "action", deny_unknown_fields)]
pub enum RoomActionSchema {
    Play,
    Pause,
    Next,
    Previous,
    Seek { to: f32 },
}

pub struct ValidatedJson<T>(pub T);

#[async_trait]
impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let extracted_json: Json<T> = Json::from_request(req, state)
            .await
            .map_err(|_| (StatusCode::BAD_REQUEST, "JSON parse failed"))?;

        extracted_json
            .0
            .validate()
            .map_err(|_| (StatusCode::BAD_REQUEST, "Request body is invalid"))?;

        Ok(Self(extracted_json.0))
    }
}
