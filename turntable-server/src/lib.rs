use aide::{
    axum::{routing::get, ApiRouter, IntoApiResponse},
    openapi::{Components, Info, OpenApi, ReferenceOr, SecurityScheme},
};
use axum::{extract::FromRef, Extension, Json};
use std::{
    env,
    net::{Ipv6Addr, SocketAddr},
    sync::Arc,
};
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use turntable_collab::Collab;

mod auth;
mod serialized;

/// The default port the server will listen on.
pub const DEFAULT_PORT: u16 = 9050;

#[derive(Clone, FromRef)]
struct ServerContext {
    pub collab: Arc<Collab>,
}

type Router = ApiRouter<ServerContext>;
type Api = OpenApi;

/// Starts the turntable server
pub async fn run_server(collab: Collab) {
    let context = ServerContext {
        collab: Arc::new(collab),
    };

    let port = env::var("TURNTABLE_SERVER_PORT")
        .map(|x| x.parse::<u16>().expect("Port must be a number"))
        .unwrap_or(DEFAULT_PORT);

    let addr: SocketAddr = (Ipv6Addr::UNSPECIFIED, port).into();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let version_one_router = Router::new().nest("/auth", auth::router());

    let root_router = Router::new()
        .nest("/v1", version_one_router)
        .route("/api.json", get(serve_api))
        .with_state(context)
        .layer(cors);

    let listener = TcpListener::bind(&addr).await.expect("listens on address");

    let mut api = setup_api();

    axum::serve(
        listener,
        root_router
            .finish_api(&mut api)
            .layer(Extension(api))
            .into_make_service(),
    )
    .await
    .unwrap();
}

fn setup_api() -> OpenApi {
    OpenApi {
        info: Info {
            title: "turntable API".to_string(),
            description: Some("Exposes endpoints to interact with a turntable server".to_string()),
            ..Default::default()
        },
        components: Some(Components {
            security_schemes: [bearer_security()].into(),
            ..Default::default()
        }),
        security: vec![],
        ..Default::default()
    }
}

fn bearer_security() -> (String, ReferenceOr<SecurityScheme>) {
    (
        "BearerAuth".to_string(),
        ReferenceOr::Item(SecurityScheme::Http {
            scheme: "bearer".to_string(),
            bearer_format: None,
            description: None,
            extensions: [].into(),
        }),
    )
}

async fn serve_api(Extension(api): Extension<Api>) -> impl IntoApiResponse {
    Json(api)
}
