use argon2::{
    password_hash::{Encoding, SaltString},
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
};
use chrono::{Duration, Utc};
use rand::rngs::OsRng;
use std::sync::Arc;
use thiserror::Error;

use crate::{
    util::random_string, Database, DatabaseError, NewSession, NewUser, PrimaryKey, SessionData,
    UpdatedUser, UserData,
};

pub struct Auth<Db> {
    db: Arc<Db>,
    argon: Argon2<'static>,
}

#[derive(Debug, Error)]
pub enum AuthError {
    /// Username or password is incorrect
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("A superuser already exists")]
    SuperuserExists,
    /// Something else went wrong with the database
    #[error(transparent)]
    Db(DatabaseError),
    #[error("HashError: {0}")]
    HashError(String),
}

impl<Db> Auth<Db>
where
    Db: Database,
{
    const SESSION_DURATION_IN_DAYS: usize = 7;

    pub fn new(db: &Arc<Db>) -> Self {
        Self {
            db: db.clone(),
            argon: Argon2::default(),
        }
    }

    /// Logs in a user, returning a new session
    pub async fn login(&self, credentials: Credentials) -> Result<SessionData, AuthError> {
        self.clear_expired().await;

        let user = self
            .db
            .user_by_username(&credentials.username)
            .await
            .map_err(|e| match e {
                DatabaseError::NotFound {
                    resource: _,
                    identifier: _,
                } => AuthError::InvalidCredentials,
                err => AuthError::Db(err),
            })?;

        let stored_password = PasswordHash::parse(&user.password, Encoding::default())
            .map_err(|e| AuthError::HashError(e.to_string()))?;

        self.argon
            .verify_password(credentials.password.as_bytes(), &stored_password)
            .map_err(|_| AuthError::InvalidCredentials)?;

        let expires_at = Utc::now() + Duration::days(Self::SESSION_DURATION_IN_DAYS as i64);

        let new_session = NewSession {
            token: random_string(32),
            user_id: user.id,
            expires_at,
        };

        let new_session = self
            .db
            .create_session(new_session)
            .await
            .map_err(AuthError::Db)?;

        Ok(new_session)
    }

    /// Deletes the associated session, if it exists
    pub async fn logout(&self, token: &str) -> Result<(), DatabaseError> {
        self.db.delete_session_by_token(token).await
    }

    /// Creates a basic user
    pub async fn register_basic(&self, new_user: NewPlainUser) -> Result<UserData, AuthError> {
        self.create_user(NewUser {
            username: new_user.username,
            password: new_user.password,
            display_name: new_user.display_name,
            superuser: false,
        })
        .await
    }

    /// Creates a superuser, if it doesn't already exist
    pub async fn register_superuser(&self, new_user: NewPlainUser) -> Result<UserData, AuthError> {
        let has_superuser = self.db.check_for_superuser().await.map_err(AuthError::Db)?;

        if has_superuser {
            return Err(AuthError::SuperuserExists);
        }

        self.create_user(NewUser {
            username: new_user.username,
            password: new_user.password,
            display_name: new_user.display_name,
            superuser: true,
        })
        .await
    }

    /// Updates a user
    pub async fn update_user(&self, updated_user: UpdatedUser) -> Result<UserData, DatabaseError> {
        self.db.update_user(updated_user).await
    }

    /// Deletes a user completely
    pub async fn delete_user(&self, user_id: PrimaryKey) -> Result<(), DatabaseError> {
        self.db.delete_user(user_id).await
    }

    /// Returns a session if it exists
    pub async fn session(&self, token: &str) -> Result<SessionData, DatabaseError> {
        self.db.session_by_token(token).await
    }

    async fn create_user(&self, new_user: NewUser) -> Result<UserData, AuthError> {
        let salt = SaltString::generate(&mut OsRng);
        let hashed_password = self
            .argon
            .hash_password(new_user.password.as_bytes(), &salt)
            .map_err(|e| AuthError::HashError(e.to_string()))?
            .to_string();

        self.db
            .create_user(NewUser {
                username: new_user.username,
                password: hashed_password,
                display_name: new_user.display_name,
                superuser: new_user.superuser,
            })
            .await
            .map_err(AuthError::Db)
    }

    async fn clear_expired(&self) {
        self.db
            .clear_expired_sessions()
            .await
            .expect("sessions are cleared")
    }
}

#[derive(Debug)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug)]
pub struct NewPlainUser {
    pub username: String,
    pub password: String,
    pub display_name: String,
}
