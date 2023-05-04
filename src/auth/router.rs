use axum::{
    debug_handler,
    extract::State,
    routing::{get, post},
    Json,
};
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    server::{Context, Router},
    util::ApiError,
};

use super::{Session, User};

pub fn router() -> Router {
    Router::new()
        .route("/user", get(user))
        .route("/register", post(register_new_user))
        .route("/login", post(login))
}

async fn user(session: Session) -> Json<User> {
    Json(session.user)
}

#[derive(Debug, Deserialize)]
struct RegisterBody {
    username: String,
    password: String,
}

async fn register_new_user(
    State(context): Context,
    Json(body): Json<RegisterBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let user = User::create(&context.db, body.username, body.password).await?;
    let session = Session::create(&context.db, &user).await?;

    let result = json!({
        "token": session.token(),
        "user": user,
    });

    Ok((StatusCode::CREATED, Json(result)))
}

#[derive(Debug, Deserialize)]
struct LoginBody {
    username: String,
    password: String,
}

async fn login(
    State(context): Context,
    Json(body): Json<LoginBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let user = User::get(&context.db, &body.username).await?;
    let is_valid = user.validate_password(&body.password);

    if is_valid {
        let session = Session::create(&context.db, &user).await?;

        let result = json!({
            "token": session.token(),
            "user": user,
        });

        Ok((StatusCode::OK, Json(result)))
    } else {
        Err(ApiError::Unauthorized)
    }
}
