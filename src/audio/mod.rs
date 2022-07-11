use std::sync::{Arc, Mutex};

mod buffering;
mod encoding;
mod loading;
mod playback;
mod stream;

pub use buffering::*;
pub use encoding::*;
pub use loading::*;
pub use playback::*;
pub use stream::*;

pub struct AudioSystem {
    stream: Arc<AudioStream>,
    player: Arc<Mutex<Player>>,
}

impl AudioSystem {
    fn new() -> Self {
        let player = Arc::new(Mutex::new(Player::new()));
        let stream = Arc::new(AudioStream::new(player.clone()));

        Self { player, stream }
    }

    pub fn stream(&self) -> AudioBufferConsumer {
        self.stream.get_consumer()
    }

    pub fn run(&self) {
        self.stream.run();
    }
}

impl Default for AudioSystem {
    fn default() -> Self {
        Self::new()
    }
}
