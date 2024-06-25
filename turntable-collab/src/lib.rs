mod auth;
mod db;
mod input;
mod queues;
mod track;
mod util;

use std::sync::Arc;

pub use db::*;
pub use input::*;
pub use queues::*;
pub use track::*;

use turntable_core::{Ingestion, Pipeline};

/// The turntable collab system, facilitating room management, authentication, and more.
pub struct Collab<I, Db> {
    pipeline: Arc<Pipeline<I>>,
    database: Arc<Db>,
}

/// A type passed to various components of the collab system, to access state, emit events, and dispatch actions.
pub struct CollabContext<I, Db> {
    pub pipeline: Arc<Pipeline<I>>,
    pub database: Arc<Db>,
}

impl<I, Db> Collab<I, Db>
where
    I: Ingestion,
    Db: Database,
{
    pub fn new(pipeline: Pipeline<I>, database: Db) -> Self {
        Self {
            pipeline: pipeline.into(),
            database: database.into(),
        }
    }
}

impl<I, Db> Clone for CollabContext<I, Db>
where
    I: Ingestion,
    Db: Database,
{
    fn clone(&self) -> Self {
        Self {
            database: self.database.clone(),
            pipeline: self.pipeline.clone(),
        }
    }
}
