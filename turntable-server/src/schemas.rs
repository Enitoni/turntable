use aide::OperationInput;
use axum::{
    async_trait,
    extract::{FromRequest, Request},
    http::StatusCode,
    Json,
};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize};
use validator::Validate;

#[derive(Debug, JsonSchema, Validate, Deserialize)]
pub struct LoginSchema {
    username: String,
    password: String,
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

impl<T> OperationInput for ValidatedJson<T> {}

impl<T> JsonSchema for ValidatedJson<T>
where
    T: JsonSchema,
{
    fn schema_name() -> String {
        T::schema_name()
    }

    fn json_schema(gen: &mut SchemaGenerator) -> Schema {
        T::json_schema(gen)
    }
}
