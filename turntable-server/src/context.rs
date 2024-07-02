use std::sync::Arc;

use axum::extract::FromRef;
use turntable_collab::Collab;

#[derive(Clone, FromRef)]
pub struct ServerContext {
    pub collab: Arc<Collab>,
}
