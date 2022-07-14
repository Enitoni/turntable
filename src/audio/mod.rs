use std::{
    path::Path,
    sync::{Arc, Mutex},
};

mod buffering;
mod decoding;
mod encoding;
mod events;
mod loading;
mod playback;
mod queuing;
mod source;
mod stream;
mod track;

pub use buffering::*;
pub use encoding::*;
pub use events::*;
pub use loading::*;
pub use playback::*;
pub use source::AudioSource;
pub use stream::*;
pub use track::Track;

pub type Sample = f32;
pub const SAMPLE_IN_BYTES: usize = 4;

pub struct AudioSystem {
    events: AudioEventChannel,
    stream: Arc<AudioStream>,
    player: Arc<Mutex<Player>>,
}

impl AudioSystem {
    fn new() -> Self {
        let events = AudioEventChannel::new();
        let player = Arc::new(Mutex::new(Player::new(events.clone())));

        {
            let mut player_guard = player.lock().unwrap();

            /*let tracks: Vec<_> = [
                "first_steps.mp3",
                "friends.mp3",
                "need_to_know.flac",
                "submersion.wav",
                "dessert.mp3",
                "getintoit.flac",
                "temple.mp3",
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
                //player_guard.add(track)
            }*/
        }

        let stream = Arc::new(AudioStream::new(player.clone()));

        Self {
            events,
            player,
            stream,
        }
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
