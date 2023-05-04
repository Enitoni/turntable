use crate::{db::Database, util::ApiError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use surrealdb::sql::Thing;

use scrypt::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Scrypt,
};

/// A user that can access Vinyl, create rooms, and so on, given they have permissions.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    pub id: Option<Thing>,
    pub username: String,

    #[serde(skip_serializing)]
    pub password: String,
    pub display_name: String,
}

impl User {
    pub async fn create(
        db: &Database,
        username: String,
        password: String,
    ) -> Result<Self, ApiError> {
        let salt = SaltString::generate(&mut OsRng);

        let hashed_password = Scrypt
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| ApiError::Other(e.into()))?
            .to_string();

        let user: User = db
            .create("user")
            .content(json!( {
                "id": username,
                "username": username.clone(),
                "password": hashed_password,
                "display_name": username,
            }))
            .await
            .map_err(ApiError::from_db)?;

        Ok(user)
    }

    pub async fn get(db: &Database, username: &str) -> Result<Self, ApiError> {
        db.query("SELECT * FROM user WHERE username = $username")
            .bind(("username", username))
            .await?
            .take::<Option<Self>>(0)?
            .ok_or(ApiError::NotFound("user"))
    }

    pub fn validate_password(&self, incoming: &str) -> bool {
        let hashed = PasswordHash::new(&self.password).expect("create password hash");
        Scrypt.verify_password(incoming.as_bytes(), &hashed).is_ok()
    }
}
