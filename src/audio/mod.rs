use std::{
    fs::{self, File},
    path::Path,
    sync::{Arc, Mutex},
};

mod buffering;
mod decoding;
mod encoding;
mod events;
mod loading;
mod playback;
mod processing;
mod queuing;
mod source;
mod stream;
mod track;

pub use buffering::*;
pub use decoding::raw_samples_from_bytes;
pub use encoding::*;
pub use events::*;
pub use loading::*;
pub use playback::*;
pub use queuing::Queue;
pub use source::{AudioSource, Error as SourceError};
pub use stream::*;
pub use track::Track;

use self::decoding::decode_to_raw;

pub type Sample = f32;
pub const SAMPLE_IN_BYTES: usize = 4;

pub struct AudioSystem {
    events: AudioEventChannel,
    stream: Arc<AudioStream>,
    queue: Arc<Queue>,
    player: Arc<Mutex<Player>>,
}

impl AudioSystem {
    fn new() -> Self {
        let events = AudioEventChannel::new();

        let queue: Arc<_> = Queue::new(events.clone()).into();
        let player: Arc<_> = Mutex::new(Player::new(events.clone(), queue.clone())).into();

        {
            let tracks: Vec<_> = fs::read_dir("./assets/trance")
                .unwrap()
                .into_iter()
                .map(|x| {
                    let entry = x.unwrap();

                    let path = entry.path();
                    let name = path.file_name().unwrap().to_str().unwrap();

                    let path = decode_to_raw(File::open(&path).unwrap(), name);
                    let source = source::FileSource::new(path);

                    Track::new(source)
                })
                .collect();

            for track in tracks {
                queue.add_track(track, queuing::QueuePosition::Add)
            }
        }

        let stream = Arc::new(AudioStream::new(player.clone()));

        Self {
            events,
            player,
            stream,
            queue,
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
