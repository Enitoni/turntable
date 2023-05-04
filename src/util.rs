use axum::response::IntoResponse;
use hyper::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0} does not exist")]
    NotFound(&'static str),

    #[error("{0} already exists")]
    Conflict(&'static str),

    #[error("Invalid credentials")]
    Unauthorized,

    #[error(transparent)]
    Database(#[from] surrealdb::Error),

    #[error("{0}")]
    Other(Box<dyn std::error::Error>),

    #[error("Unknown error")]
    Unknown,
}

use surrealdb::error::Api as ApiErr;
use surrealdb::error::Db as DbErr;
use surrealdb::Error as SurrealErr;

impl ApiError {
    pub fn from_db(err: SurrealErr) -> Self {
        match err {
            SurrealErr::Db(x) => match x {
                DbErr::RecordExists { thing: _ } => Self::Conflict("Resource"),
                x => Self::Database(SurrealErr::Db(x)),
            },
            SurrealErr::Api(err) => Self::from_db_api(err),
        }
    }

    fn from_db_api(err: ApiErr) -> Self {
        match err {
            ApiErr::Query(x) => {
                // Sadly, SurrealDB does not have a proper error for this
                // when interacting via a remote instance.
                if x.contains("already exists") {
                    return Self::Conflict("Resource");
                }

                Self::Database(SurrealErr::Api(ApiErr::Query(x)))
            }
            x => Self::Database(SurrealErr::Api(x)),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, self.to_string()).into_response()
    }
}

pub mod sync {
    use std::fmt::Debug;

    use crossbeam::channel::{unbounded, Receiver, Sender};

    /// A helper struct to wait for something to happen
    pub struct Wait {
        sender: Sender<()>,
        receiver: Receiver<()>,
    }

    impl Wait {
        pub fn wait(&self) {
            let _ = self.receiver.recv();
        }

        pub fn notify(&self) {
            self.sender.send(()).unwrap();
        }
    }

    impl Default for Wait {
        fn default() -> Self {
            let (sender, receiver) = unbounded();
            Self { sender, receiver }
        }
    }

    impl Debug for Wait {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Wait")
        }
    }
}
