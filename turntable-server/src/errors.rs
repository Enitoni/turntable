use aide::OperationOutput;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;
use turntable_collab::{AuthError, DatabaseError};

pub type ServerResult<T> = Result<T, ServerError>;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("{resource}:{identifier} not found")]
    NotFound {
        resource: &'static str,
        identifier: &'static str,
    },
    #[error("{resource} with {field} of value {value} already exists")]
    Conflict {
        resource: &'static str,
        field: &'static str,
        value: String,
    },
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("A superuser already exists")]
    SuperuserExists,
    #[error("Unknown internal error: {0}")]
    Unknown(String),
}

impl ServerError {
    fn as_status_code(&self) -> StatusCode {
        match self {
            Self::SuperuserExists => StatusCode::CONFLICT,
            Self::InvalidCredentials => StatusCode::BAD_REQUEST,
            Self::Conflict {
                resource: _,
                field: _,
                value: _,
            } => StatusCode::CONFLICT,
            Self::NotFound {
                resource: _,
                identifier: _,
            } => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        (self.as_status_code(), self.to_string()).into_response()
    }
}

impl OperationOutput for ServerError {
    type Inner = Self;
}

impl From<AuthError> for ServerError {
    fn from(value: AuthError) -> Self {
        match value {
            AuthError::InvalidCredentials => Self::InvalidCredentials,
            AuthError::SuperuserExists => Self::SuperuserExists,
            e => Self::Unknown(e.to_string()),
        }
    }
}

impl From<DatabaseError> for ServerError {
    fn from(value: DatabaseError) -> Self {
        match value {
            DatabaseError::NotFound {
                resource,
                identifier,
            } => Self::NotFound {
                resource,
                identifier,
            },
            DatabaseError::Conflict {
                resource,
                field,
                value,
            } => Self::Conflict {
                resource,
                field,
                value,
            },
            e => Self::Unknown(e.to_string()),
        }
    }
}
