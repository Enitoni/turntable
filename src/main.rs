use std::{env, sync::Arc};

use turntable_collab::Collab;
use turntable_core::Config;
use turntable_server::run_server;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    println!("Setting up Collab...");

    let collab = Arc::new(Collab::new(Config::default(), &database_url).await);

    println!("Server running.");
    run_server(&collab).await
}
