use std::sync::Arc;

mod stream;
pub use stream::*;

pub struct AudioSystem {
    stream: Arc<AudioStream>,
}

impl AudioSystem {
    fn new() -> Self {
        Self {
            stream: Arc::new(AudioStream::new()),
        }
    }

    pub fn stream(&self) -> AudioStreamConsumer {
        self.stream.read()
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
