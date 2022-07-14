use std::sync::{Arc, Mutex};

use crossbeam::channel::{unbounded, Receiver, Sender};

use super::{queuing::QueueEvent, LoaderEvent};

#[derive(Debug, Clone)]
pub enum AudioEvent {
    Queue(QueueEvent),
    Loader(LoaderEvent),
}

#[derive(Debug)]
pub struct AudioEventChannel {
    id: usize,
    broadcaster: Arc<AudioEventBroadcast>,
    receiver: Receiver<AudioEvent>,
}

#[derive(Debug)]
pub struct AudioEventBroadcast {
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

#[cfg(test)]
mod test {
    use std::{thread, time::Duration};

    use super::{AudioEvent, AudioEventChannel};
    use crate::audio::LoaderEvent;

    #[test]
    fn it_consumes_multiple_places() {
        let events = AudioEventChannel::new();

        thread::spawn({
            let one = events.clone();

            move || loop {
                let event = one.wait();
                dbg!("woo", event);
            }
        });

        thread::spawn({
            let two = events.clone();

            move || loop {
                let event = two.wait();
                dbg!("waa", event);
            }
        });

        events.emit(AudioEvent::Loader(LoaderEvent::Advance));
        thread::sleep(Duration::from_secs(2));
    }

    #[test]
    fn it_drops_correctly() {
        let events = AudioEventChannel::new();

        {
            events.clone();
            events.clone();
        }
        {
            let a = events.clone();
            let b = a.clone();

            thread::spawn(move || {
                let c = b.clone();
            });
        }
        {
            events.clone();
            events.clone();
        }
        {
            events.clone();
        }

        thread::sleep(Duration::from_millis(100));
        let clones = events.broadcaster.channels.lock().unwrap().len();
        assert_eq!(clones, 1);
    }
}
