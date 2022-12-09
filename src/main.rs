use audio::Input;
use std::{sync::Arc, thread, time::Duration};
use tokio::runtime::Runtime;

mod audio;
mod http;
mod logging;
mod util;
mod ytdl;

fn main() {
    logging::init_logger();

    let audio = Arc::new(audio::AudioSystem::default());
    audio.start();

    let runtime = Runtime::new().unwrap();

    let input = Input::parse("https://www.youtube.com/watch?v=xsgnpOnV58k").unwrap();
    audio.add(input);

    thread::spawn({
        let http_audio = Arc::clone(&audio);

        move || http::run(http_audio)
    });

    // Run forever
    loop {
        let time_to_sleep = Duration::from_secs(60);
        thread::sleep(time_to_sleep);
    }
}
