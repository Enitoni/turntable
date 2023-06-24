use super::{OrderStrategy, Queue, QueueEvent, QueueId, QueueItem, SerializedQueue, SubQueueId};
use crate::{
    audio::{AudioEvent, PlayerId},
    auth::User,
    events::Handler,
    store::Store,
    track::Track,
    EventEmitter, VinylEvent,
};
use dashmap::DashMap;
use std::sync::{Arc, Weak};

#[derive(Debug)]
pub struct QueueStore {
    store: Weak<Store>,
    emitter: EventEmitter,

    queues: DashMap<QueueId, Queue>,
    players: DashMap<QueueId, PlayerId>,
}

impl QueueStore {
    pub fn new(store: Weak<Store>, emitter: EventEmitter) -> Self {
        Self {
            store,
            emitter,
            queues: Default::default(),
            players: Default::default(),
        }
    }

    /// Create a new queue and associate a player to apply it to
    pub fn create_queue(&self, player: PlayerId) -> QueueId {
        let queue = Queue::new();
        let id = queue.id;

        self.queues.insert(id, queue);
        self.players.insert(id, player);

        id
    }

    pub fn create_sub_queue(&self, queue: QueueId, owner: User) -> SubQueueId {
        self.queues
            .get(&queue)
            .expect("queue exists")
            .create_sub_queue(owner, OrderStrategy::Interleave)
    }

    pub fn add(&self, sub_queue: SubQueueId, tracks: Vec<Track>) {
        let queue = self
            .queues
            .iter()
            .find(|q| q.has_sub_queue(sub_queue))
            .expect("queue exists");

        queue.add(sub_queue, tracks);

        self.apply_to_player(queue.id);
        self.emitter.dispatch(QueueEvent::Update {
            queue: queue.id,
            new_items: queue.items(),
        });
    }

    pub fn next(&self, queue: QueueId) {
        let item = self.queues.get(&queue).expect("queue exists").next();

        self.apply_to_player(queue);

        if let Some(item) = item {
            self.emitter.dispatch(QueueEvent::Advance { queue, item });
        }
    }

    pub fn current_item(&self, queue: QueueId) -> Option<QueueItem> {
        self.queues
            .get(&queue)
            .expect("queue exists")
            .current_item()
    }

    pub fn serialized(&self, queue: QueueId) -> SerializedQueue {
        SerializedQueue::new(&self.queues.get(&queue).expect("queue exists"))
    }

    /// Applies the queue to the player, ensuring tracks are activated
    fn apply_to_player(&self, queue_id: QueueId) {
        let store = self.store();
        let queue = self.queues.get(&queue_id).expect("queue exists");

        let player = self
            .players
            .get(&queue_id)
            .expect("player is assigned")
            .upgrade(&store);

        let tracks = queue.tracks_to_play();

        for track in tracks.iter() {
            let result = track.ensure_activation(&store.ingestion);

            if result.is_err() {
                self.emitter.dispatch(QueueEvent::ActivationError {
                    queue: queue_id,
                    track: track.id,
                })
            }
        }

        let sinks: Vec<_> = tracks
            .into_iter()
            .flat_map(|x| x.sink())
            .map(|x| x.upgrade(&store))
            .collect();

        player.set_sinks(sinks);
    }

    fn store(&self) -> Arc<Store> {
        self.store.upgrade().unwrap()
    }

    pub fn handler(&self) -> QueueHandler {
        QueueHandler {
            store: self.store.clone(),
        }
    }
}

pub struct QueueHandler {
    store: Weak<Store>,
}

impl Handler<VinylEvent> for QueueHandler {
    type Incoming = AudioEvent;

    fn handle(&self, incoming: Self::Incoming) {
        let store = self.store.upgrade().unwrap();

        if let AudioEvent::Next { player } = incoming {
            let queue = store
                .queue_store
                .players
                .iter()
                .find_map(|x| (x.value() == &player).then_some(*x.key()))
                .expect("queue exists");

            store.queue_store.next(queue)
        }
    }
}
