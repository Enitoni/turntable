use aide::{
    axum::{routing::get, ApiRouter, IntoApiResponse},
    openapi::{Info, OpenApi},
};
use axum::{Extension, Json};
use std::{
    env,
    net::{Ipv6Addr, SocketAddr},
};
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};

/// The default port the server will listen on.
pub const DEFAULT_PORT: u16 = 9050;

pub type Router = ApiRouter<()>;
pub type Api = OpenApi;

/// Starts the turntable server
pub async fn run_server() {
    let port = env::var("TURNTABLE_SERVER_PORT")
        .map(|x| x.parse::<u16>().expect("Port must be a number"))
        .unwrap_or(DEFAULT_PORT);

    let addr: SocketAddr = (Ipv6Addr::UNSPECIFIED, port).into();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let version_one_router = Router::new();

    let root_router = Router::new()
        .nest("/v1", version_one_router)
        .route("/api.json", get(serve_api))
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
        ..Default::default()
    }
}

async fn serve_api(Extension(api): Extension<Api>) -> impl IntoApiResponse {
    Json(api)
}
