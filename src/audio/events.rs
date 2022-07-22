use std::sync::{Arc, Mutex};

use crossbeam::channel::{unbounded, Receiver, Sender};

use super::queuing::QueueEvent;

#[derive(Debug, Clone)]
pub enum AudioEvent {
    Queue(QueueEvent),
}

#[derive(Debug)]
pub struct AudioEventChannel {
    id: usize,
    broadcaster: Arc<AudioEventBroadcast>,
    receiver: Receiver<AudioEvent>,
}

#[derive(Debug)]
struct AudioEventBroadcast {
    channels: Mutex<Vec<(usize, Sender<AudioEvent>)>>,
}

impl AudioEventChannel {
    pub fn new() -> Self {
        // Hahahahaha
        AudioEventBroadcast::channel(Arc::new(AudioEventBroadcast::new()))
    }

    fn internal_new(
        id: usize,
        broadcaster: Arc<AudioEventBroadcast>,
    ) -> (Self, Sender<AudioEvent>) {
        let (sender, receiver) = unbounded();
        let channel = Self {
            id,
            broadcaster,
            receiver,
        };

        (channel, sender)
    }

    pub fn emit<T: Into<AudioEvent>>(&self, event: T) {
        self.broadcaster.broadcast(event.into());
    }

    pub fn wait(&self) -> AudioEvent {
        self.receiver.recv().expect("No error on event")
    }
}

impl AudioEventBroadcast {
    fn new() -> Self {
        Self {
            channels: Default::default(),
        }
    }

    fn channel(broadcaster: Arc<Self>) -> AudioEventChannel {
        let mut channels = broadcaster.channels.lock().unwrap();

        let id = channels.iter().fold(0, |acc, c| acc + c.0) + 1;
        let (new_channel, sender) = AudioEventChannel::internal_new(id, broadcaster.clone());

        channels.push((id, sender));
        new_channel
    }

    fn remove(&self, id: usize) {
        let mut channels = self.channels.lock().unwrap();

        *channels = channels.iter().filter(|c| c.0 != id).cloned().collect();
    }

    fn broadcast(&self, event: AudioEvent) {
        let channels = self.channels.lock().unwrap();

        for (_, sender) in channels.iter() {
            sender.send(event.clone()).expect("Broadcasts event");
        }
    }
}

impl Drop for AudioEventChannel {
    fn drop(&mut self) {
        self.broadcaster.remove(self.id);
    }
}

impl Clone for AudioEventChannel {
    fn clone(&self) -> Self {
        AudioEventBroadcast::channel(self.broadcaster.clone())
    }
}
