use std::borrow::BorrowMut;

use axum::{response::IntoResponse, Json};
use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    Modify, OpenApi,
};
use utoipauto::utoipauto;

#[utoipauto(paths = "./turntable-server/src")]
#[derive(OpenApi)]
#[openapi(
    modifiers(&Security),
    info(
        description = "turntable-server exposes endpoints to interact with this turntable instance"
    ))
]
pub struct ApiDoc;

struct Security;

impl Modify for Security {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.borrow_mut() {
            let scheme = HttpBuilder::new()
                .scheme(HttpAuthScheme::Bearer)
                .bearer_format("Bearer <token>")
                .build();

            components.add_security_scheme("BearerAuth", SecurityScheme::Http(scheme))
        }
    }
}

pub async fn docs() -> impl IntoResponse {
    Json(ApiDoc::openapi())
}
