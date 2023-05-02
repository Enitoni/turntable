use std::sync::Arc;

use audio::AudioSystem;
use server::ws::WebSocketManager;
use tokio::runtime::{self, Runtime};

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

    runtime: Runtime,
}

#[derive(Clone)]
pub struct VinylContext {
    pub audio: Arc<AudioSystem>,
    pub websockets: Arc<WebSocketManager>,
}

impl Vinyl {
    fn new() -> Self {
        let main_runtime = runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("vinyl-async")
            .build()
            .expect("async runtime built");

        Self {
            audio: AudioSystem::new(),
            websockets: WebSocketManager::new(),
            runtime: main_runtime,
        }
    }

    fn run(&self) {
        audio::spawn_audio_thread(self.audio.clone());

        self.runtime
            .block_on(async move { server::run_server(self.context()).await })
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
