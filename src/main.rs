use std::sync::Arc;

use audio::AudioSystem;
use server::ws::WebSocketManager;

mod audio;
mod http;
mod ingest;
mod logging;
mod server;
mod util;

pub struct Vinyl {
    // This is temporary for now, since Rooms will have their own audio system
    audio: Arc<AudioSystem>,
    websockets: Arc<WebSocketManager>,
}

#[derive(Clone)]
pub struct VinylContext {
    pub audio: Arc<AudioSystem>,
    pub websockets: Arc<WebSocketManager>,
}

impl Vinyl {
    fn new() -> Self {
        Self {
            audio: AudioSystem::new(),
            websockets: WebSocketManager::new(),
        }
    }

    fn run(&self) {
        audio::spawn_audio_thread(self.audio.clone());
        server::run_server(self.context()).join().unwrap();
    }

    fn context(&self) -> VinylContext {
        VinylContext {
            audio: self.audio.clone(),
            websockets: self.websockets.clone(),
        }
    }
}

fn main() {
    logging::init_logger();

    let vinyl = Vinyl::new();
    vinyl.run();
}
