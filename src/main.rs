use log::{error, info, warn};
use std::{sync::Arc, thread, time::Duration};
use tokio::runtime::Runtime;

mod audio;
mod discord;
mod http;
mod logging;
mod util;

fn main() {
    logging::init_logger();

    let audio = Arc::new(audio::AudioSystem::default());
    audio.run();

    let runtime = Runtime::new().unwrap();
    let runtime_handle = runtime.handle();

    let http_handle = thread::spawn({
        let http_audio = Arc::clone(&audio);

        move || http::run(http_audio)
    });

    let bot_audio = audio;
    let bot_handle = runtime_handle.spawn(async move {
        discord::Bot::run(bot_audio).await;
    });

    // Run forever
    loop {
        let time_to_sleep = Duration::from_millis(50);
        thread::sleep(time_to_sleep);

        if bot_handle.is_finished() && http_handle.is_finished() {
            break;
        }
    }
}
