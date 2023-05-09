use crossbeam::channel::{unbounded, Receiver, Sender};
use serde::Serialize;
use tokio::task::spawn_blocking;

use crate::{
    audio::Track,
    auth::{User, UserId},
    rooms::RoomId,
    server::ws::Recipients,
    VinylContext,
};

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "kebab-case")]
pub enum Event {
    /// A new user connected the room stream
    UserEnteredRoom {
        user: User,
        room: RoomId,
    },
    /// A user disconnected from the room stream
    UserLeftRoom {
        user: UserId,
        room: RoomId,
    },
    /// The current track in a room changed
    TrackUpdate {
        room: RoomId,
        track: Track,
    },
    QueueAdd {
        user: UserId,
        track: Track,
    },
    /// Scheduler read a sink and set a new offset
    PlayerTime {
        room: RoomId,
        seconds: f32,
    },
}

type Message = (Event, Recipients);

#[derive(Debug, Clone)]
pub struct Events {
    sender: Sender<Message>,
    receiver: Receiver<Message>,
}

impl Events {
    pub fn emit(&self, event: Event, recipients: Recipients) {
        self.sender.send((event, recipients)).unwrap();
    }
}

impl Default for Events {
    fn default() -> Self {
        let (sender, receiver) = unbounded();
        Self { sender, receiver }
    }
}

pub async fn check_events(context: VinylContext) {
    while let Ok((event, recipients)) = {
        let receiver = context.events.receiver.clone();
        spawn_blocking(move || receiver.recv()).await.unwrap()
    } {
        let message = serde_json::to_string(&event).expect("serialize event");
        context.websockets.broadcast(message, recipients).await
    }
}
