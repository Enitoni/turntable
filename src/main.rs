use std::{sync::Arc, thread, time::Duration};

use audio::{spawn_audio_thread, AudioSystem};

mod audio;
mod http;
mod ingest;
mod logging;
mod server;
mod util;

fn main() {
    logging::init_logger();

    let audio_system = Arc::new(AudioSystem::default());
    spawn_audio_thread(audio_system);

    let server_thread = server::start_server();
    server_thread.join().unwrap();
}
