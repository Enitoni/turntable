mod queue;
mod queue_item;

use std::{sync::Arc, thread};

use crossbeam::channel::{unbounded, Receiver, Sender};
pub use queue::*;
pub use queue_item::*;

use crate::{
    get_or_create_handle, Ingestion, PipelineAction, PipelineContext, PipelineEvent, PlayerId,
};

/// A type passed to a queue to allow it to notify the Pipeline that it changed.
pub struct QueueNotifier {
    context: PipelineContext,
    player_id: PlayerId,
}

impl QueueNotifier {
    /// Create a new QueueNotifier.
    pub fn new(context: &PipelineContext, player_id: PlayerId) -> Self {
        QueueNotifier {
            context: context.clone(),
            player_id,
        }
    }

    pub fn notify(&self) {
        self.context.dispatch(PipelineAction::NotifyQueueUpdate {
            player_id: self.player_id,
        });
    }
}

pub struct Queuing {
    context: PipelineContext,
    sender: Sender<PlayerId>,
}

impl Queuing {
    pub fn new<I>(context: &PipelineContext, ingestion: Arc<I>) -> Self
    where
        I: Ingestion + 'static,
    {
        let context = context.clone();
        let (sender, receiver) = unbounded();

        spawn_update_task_thread(&context, ingestion, receiver);

        Self { context, sender }
    }

    /// Creates a new queue for a player.
    pub fn create_queue<T>(&self, player_id: PlayerId) -> Arc<T>
    where
        T: Queue,
    {
        let notifier = QueueNotifier::new(&self.context, player_id);

        // This is kinda cursed, but I do not know how to get around it at this point in time.
        // If this is to be changed, the entire queue system would probably need to be rewritten.
        let arced_queue = Arc::new(T::new(notifier));
        let boxed_queue = BoxedQueue::new(arced_queue.clone());

        self.context.queues.insert(player_id, boxed_queue);
        arced_queue
    }

    /// Notifies the queue system that a queue has been updated.
    pub fn notify_queue_update(&self, player_id: PlayerId) {
        self.sender.send(player_id).unwrap();
    }
}

fn spawn_update_task_thread<I>(
    context: &PipelineContext,
    ingestion: Arc<I>,
    receiver: Receiver<PlayerId>,
) where
    I: Ingestion + 'static,
{
    let handle = get_or_create_handle();
    let context = context.clone();

    let run = move || loop {
        let player_id = receiver.recv().unwrap();
        let fut = update_sinks(&context, ingestion.clone(), player_id);

        handle.block_on(fut)
    };

    thread::spawn(run);
}

/// Called when a queue is updated.
async fn update_sinks<I>(context: &PipelineContext, ingestion: Arc<I>, player_id: PlayerId)
where
    I: Ingestion + 'static,
{
    let queue = context.queues.get(&player_id).expect("queue exists");
    let player = context.players.get(&player_id).expect("player exists");
    let items = queue.peek();

    // If there's nothing in the queue, we don't need to do anything.
    if items.is_empty() {
        return;
    }

    // If the item has no length, we don't need more than one sink.
    let mut remaining_length = context.config.preload_size_in_seconds;
    let mut new_sinks = vec![];

    // We don't need to preload sinks past an infinite/unknown length one.
    if let Some(length) = items[0].length() {
        remaining_length -= length;
    } else {
        remaining_length = 0.;
    }

    for item in items {
        let existing_sink = item.sink_id().and_then(|id| context.sinks.get(&id));

        // We activate an item only if it doesn't already have a sink associated with it.
        if let Some(sink) = existing_sink {
            new_sinks.push(sink.clone());
        } else {
            let loader = item.loadable().await;

            let sink = match loader {
                Ok(loader) => ingestion.ingest(loader).await,
                Err(err) => Err(err),
            };

            match sink {
                Ok(sink) => {
                    item.register_sink(sink.id);

                    context.emit(PipelineEvent::QueueItemActivated {
                        player_id,
                        new_sink_id: sink.id,
                        item_id: item.item_id(),
                    });

                    new_sinks.push(sink);
                    remaining_length -= item.length().unwrap_or_default();
                }
                Err(err) => {
                    let item_id = item.item_id();

                    queue.skip(&item_id);
                    context.emit(PipelineEvent::QueueItemActivationError {
                        player_id,
                        item_id,
                        error: err.to_string(),
                    });
                }
            }
        }

        // Always activate at least two ahead.
        if remaining_length <= 0. && new_sinks.len() >= 3 {
            break;
        }
    }

    player.set_sinks(new_sinks);
}
