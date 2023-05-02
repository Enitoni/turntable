use serde::Deserialize;
use surrealdb::{
    engine::remote::ws::{Client, Ws},
    opt::auth::Root,
    sql::Thing,
    Surreal,
};
use thiserror::Error as ThisError;

pub type Database = Surreal<Client>;

#[derive(Debug, ThisError)]
pub enum Error {
    #[error(transparent)]
    Internal(#[from] surrealdb::Error),

    #[error("The {0} already exists")]
    Conflict(String),

    #[error("The {0} does not exist")]
    NotFound(String),

    #[error("Unknown database error")]
    Unknown,
}

pub async fn connect() -> Result<Database, surrealdb::Error> {
    let db = Surreal::new::<Ws>("127.0.0.1:8000").await?;

    db.signin(Root {
        username: "root",
        password: "root",
    })
    .await?;

    db.use_ns("vinyl").use_db("main").await?;

    Ok(db)
}

#[derive(Debug, Deserialize)]
pub struct Record {
    #[allow(dead_code)]
    id: Thing,
}
