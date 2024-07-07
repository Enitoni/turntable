use std::{env, sync::Arc};

use turntable_collab::Collab;
use turntable_core::Config;
use turntable_server::run_server;

mod logging;

/// The default port the server will listen on.
pub const DEFAULT_PORT: u16 = 9050;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    logging::init_logger();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    println!("Setting up Collab...");

    let collab = Arc::new(Collab::new(Config::default(), &database_url).await);

    let port = env::var("TURNTABLE_SERVER_PORT")
        .map(|x| x.parse::<u16>().expect("Port must be a number"))
        .unwrap_or(DEFAULT_PORT);

    println!("Server running on http://localhost:{}", port);
    run_server(&collab, port).await
}
