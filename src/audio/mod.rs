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

    pub fn stream(&self) -> AudioStreamRef {
        AudioStreamRef::new(self.stream.clone())
    }
}

impl Default for AudioSystem {
    fn default() -> Self {
        Self::new()
    }
}
