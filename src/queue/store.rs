use super::{OrderStrategy, Queue, QueueEvent, QueueId, SubQueueId};
use crate::{audio::PlayerId, auth::User, store::Store, track::Track, EventEmitter};
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
        self.emitter.dispatch(QueueEvent::Advance { queue, item });
    }

    pub fn current_track(&self, queue: QueueId) -> Option<Track> {
        let queue = self.queues.get(&queue).expect("queue exists");
        queue.tracks_to_play().into_iter().next()
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
            // TODO: This expect should be remove in the future
            track
                .ensure_activation(&store.ingestion)
                .expect("activates track")
        }

        let sinks: Vec<_> = tracks
            .into_iter()
            .map(|x| x.sink().unwrap().upgrade(&store))
            .collect();

        player.set_sinks(sinks);
    }

    fn store(&self) -> Arc<Store> {
        self.store.upgrade().unwrap()
    }
}
