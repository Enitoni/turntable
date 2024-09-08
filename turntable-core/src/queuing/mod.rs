mod queue;
mod queue_item;

use std::{sync::Arc, thread};

use crossbeam::channel::{unbounded, Receiver, Sender};
pub use queue::*;
pub use queue_item::*;

use crate::{
    get_or_create_handle, Ingestion, PipelineAction, PipelineContext, PlayerId, Sink, SinkManager,
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
    pub fn new<I>(context: &PipelineContext, manager: Arc<SinkManager<I>>) -> Self
    where
        I: Ingestion + 'static,
    {
        let context = context.clone();
        let (sender, receiver) = unbounded();

        spawn_update_task_thread(&context, manager, receiver);

        Self { context, sender }
    }

    /// Creates a new queue for a player.
    pub fn create_queue<T, F>(&self, player_id: PlayerId, creator: F) -> Arc<T>
    where
        T: Queue,
        F: FnOnce(QueueNotifier) -> T,
    {
        let notifier = QueueNotifier::new(&self.context, player_id);
        let queue = creator(notifier);

        // This is kinda cursed, but I do not know how to get around it at this point in time.
        // If this is to be changed, the entire queue system would probably need to be rewritten.
        let arced_queue = Arc::new(queue);
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
    manager: Arc<SinkManager<I>>,
    receiver: Receiver<PlayerId>,
) where
    I: Ingestion + 'static,
{
    let handle = get_or_create_handle();
    let context = context.clone();

    let run = move || loop {
        let player_id = receiver.recv().unwrap();
        let fut = update_sinks(&context, manager.clone(), player_id);

        handle.block_on(fut)
    };

    thread::Builder::new()
        .name("queue-update-task".to_string())
        .spawn(run)
        .expect("queue-update-task thread is spawned");
}

/// Called when a queue is updated.
async fn update_sinks<I>(
    context: &PipelineContext,
    manager: Arc<SinkManager<I>>,
    player_id: PlayerId,
) where
    I: Ingestion + 'static,
{
    let queue = context.queues.get(&player_id).expect("queue exists");
    let player = context.players.get(&player_id).expect("player exists");
    let items = queue.peek();

    // If there's nothing in the queue, we don't need to do anything.
    if items.is_empty() {
        return;
    }

    let sinks_to_play = ensure_sinks_for_items(context, &items, &manager);
    player.set_sinks(sinks_to_play);

    let context = context.clone();
    activate_necessary_items(context, items, manager).await;
}

/// Ensures that all the items have an associated sink before activation
fn ensure_sinks_for_items<I>(
    context: &PipelineContext,
    items: &[BoxedQueueItem],
    manager: &Arc<SinkManager<I>>,
) -> Vec<Arc<Sink>>
where
    I: Ingestion + 'static,
{
    items
        .iter()
        .map(|i| {
            i.sink_id()
                .and_then(|id| context.sinks.get(&id).map(|s| s.clone()))
                .unwrap_or_else(|| {
                    let sink = manager.prepare();
                    i.register_sink(sink.id);

                    sink
                })
        })
        .collect()
}

/// Activates items as necessary
async fn activate_necessary_items<I>(
    context: PipelineContext,
    items: Vec<BoxedQueueItem>,
    manager: Arc<SinkManager<I>>,
) where
    I: Ingestion + 'static,
{
    // If the item has no length, we don't need more than one sink.
    let mut remaining_length = context.config.preload_size_in_seconds;

    // We don't need to preload sinks past an infinite/unknown length one.
    if let Some(length) = items[0].length() {
        remaining_length -= length;
    } else {
        remaining_length = 0.;
    }

    let pairs: Vec<_> = items
        .into_iter()
        .enumerate()
        .map(|(index, i)| {
            (
                index,
                i.sink_id()
                    .and_then(|id| context.sinks.get(&id))
                    .expect("sink exists for activation")
                    .clone(),
                i,
            )
        })
        .collect();

    for (checked_sinks, sink, item) in pairs {
        // Always activate at least two ahead.
        if remaining_length <= 0. && checked_sinks >= 3 {
            break;
        }

        remaining_length -= item.length().unwrap_or_default();

        if sink.is_activatable() {
            manager.activate(sink.id, item.loadable()).await
        }
    }
}
