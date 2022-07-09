use std::sync::Arc;

mod buffering;
mod stream;

pub use buffering::*;
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
