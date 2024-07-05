use std::sync::Arc;

use turntable_collab::Collab;
use turntable_core::Config;
use turntable_server::run_server;

#[tokio::main]
async fn main() {
    println!("Setting up Collab...");

    let collab = Arc::new(
        Collab::new(
            Config::default(),
            "postgres://turntable:turntable@localhost/turntable",
        )
        .await,
    );

    println!("Server running.");
    run_server(&collab).await
}
