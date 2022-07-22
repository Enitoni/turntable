use std::{sync::Arc, thread, time::Duration};
use tokio::runtime::Runtime;

mod audio;
mod discord;
mod http;
mod logging;
mod util;
mod ytdl;

fn main() {
    logging::init_logger();

    let audio = Arc::new(audio::AudioSystem::default());
    audio.start();

    let runtime = Runtime::new().unwrap();

    thread::spawn({
        let http_audio = Arc::clone(&audio);

        move || http::run(http_audio)
    });

    runtime.handle().spawn(async move {
        discord::Bot::run(audio).await;
    });

    // Run forever
    loop {
        let time_to_sleep = Duration::from_secs(60);
        thread::sleep(time_to_sleep);
    }
}
