use std::{convert::Infallible, sync::Arc};

use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};
use turntable_collab::Collab;

#[derive(Clone, FromRef)]
pub struct ServerContext {
    pub collab: Arc<Collab>,
}

#[async_trait]
impl FromRequestParts<ServerContext> for ServerContext {
    type Rejection = Infallible;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &ServerContext,
    ) -> Result<Self, Self::Rejection> {
        let context = ServerContext::from_ref(state);

        Ok(context)
    }
}
