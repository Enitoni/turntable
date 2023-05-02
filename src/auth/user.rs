use crate::db::{Database, Error};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;

use scrypt::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Scrypt,
};

/// A user that can access Vinyl, create rooms, and so on, given they have permissions.
#[derive(Debug, Deserialize, Serialize)]
pub struct User {
    id: Option<Thing>,

    username: String,
    password: String,

    display_name: String,
}

impl User {
    pub async fn new(db: &Database, username: String, password: String) -> Result<Self> {
        let salt = SaltString::generate(&mut OsRng);

        let hashed_password = Scrypt
            .hash_password(password.as_bytes(), &salt)?
            .to_string();

        let user: User = db
            .create("user")
            .content(User {
                id: None,
                username: username.clone(),
                password: hashed_password,
                display_name: username,
            })
            .await?;

        Ok(user)
    }

    pub async fn get(db: &Database, username: String) -> Result<Self> {
        db.query("SELECT * FROM user WHERE username = $username")
            .bind(("username", username))
            .await?
            .take::<Option<Self>>(0)?
            .ok_or(Error::NotFound("user").into())
    }

    pub fn validate_password(&self, incoming: &str) -> bool {
        let hashed = PasswordHash::new(&self.password).expect("create password hash");
        Scrypt.verify_password(incoming.as_bytes(), &hashed).is_ok()
    }
}
