use std::{
    path::Path,
    sync::{Arc, Mutex},
};

mod buffering;
mod decoding;
mod encoding;
mod loading;
mod playback;
mod stream;

pub use buffering::*;
pub use encoding::*;
pub use loading::*;
pub use playback::*;
pub use stream::*;

pub type Sample = f32;

pub struct AudioSystem {
    stream: Arc<AudioStream>,
    player: Arc<Mutex<Player>>,
}

impl AudioSystem {
    fn new() -> Self {
        let player = Arc::new(Mutex::new(Player::new()));

        {
            let mut player_guard = player.lock().unwrap();

            for _ in 0..1 {
                // Temporary testing
                let track_one = Track::from_file(Path::new("./assets/blue1.wav"));
                let track_two = Track::from_file(Path::new("./assets/blue2.wav"));
                let track_three = Track::from_file(Path::new("./assets/you_right.flac"));
                player_guard.add(track_one);
                player_guard.add(track_two);
                player_guard.add(track_three);
            }
        }

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
