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

            let tracks: Vec<_> = [
                "red.mp3",
                "rise.mp3",
                "lies.mp3",
                "gregor.wav",
                "you_right.flac",
            ]
            .into_iter()
            .map(|x| {
                let path = format!("./assets/{x}");
                let path = Path::new(path.as_str());
                Track::from_file(path)
            })
            .collect();

            for track in tracks {
                player_guard.add(track)
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
