use std::sync::Arc;

mod audio;
mod discord;

#[tokio::main]
async fn main() {
    let audio = Arc::new(audio::AudioSystem::default());

    discord::Bot::run(audio).await;
}
