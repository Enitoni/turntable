use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;
use turntable_collab::{AuthError, DatabaseError, RoomError};

pub type ServerResult<T> = Result<T, ServerError>;

#[derive(Debug, Error)]
pub enum ServerError {
    // General
    #[error("{resource}:{identifier} not found")]
    NotFound {
        resource: &'static str,
        identifier: String,
    },
    #[error("{resource} with {field} of value {value} already exists")]
    Conflict {
        resource: &'static str,
        field: &'static str,
        value: String,
    },
    #[error("Unknown internal error: {0}")]
    Unknown(String),
    // Auth
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("A superuser already exists")]
    SuperuserExists,
    // Rooms
    #[error("Room is not active")]
    RoomNotActive,
    #[error("User is not a member of this room")]
    UserNotInRoom,
    #[error("User does not own this stream key")]
    StreamKeyNotOwn,
    #[error("Stream key does not exist")]
    StreamKeyNotFound,
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
            Self::RoomNotActive => StatusCode::BAD_REQUEST,
            Self::UserNotInRoom => StatusCode::FORBIDDEN,
            Self::StreamKeyNotFound => StatusCode::NOT_FOUND,
            Self::StreamKeyNotOwn => StatusCode::FORBIDDEN,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        (self.as_status_code(), self.to_string()).into_response()
    }
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
                identifier: identifier.to_string(),
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

impl From<RoomError> for ServerError {
    fn from(value: RoomError) -> Self {
        match value {
            RoomError::RoomNotFound(identifer) => Self::NotFound {
                resource: "room",
                identifier: identifer,
            },
            RoomError::RoomNotActive => Self::RoomNotActive,
            RoomError::UserNotInRoom => Self::UserNotInRoom,
            RoomError::StreamKeyNotFound => Self::StreamKeyNotFound,
            RoomError::StreamKeyNotOwn => Self::StreamKeyNotOwn,
            e => Self::Unknown(e.to_string()),
        }
    }
}
