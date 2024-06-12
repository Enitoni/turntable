use async_trait::async_trait;

use crate::{BoxedLoadable, SinkId};

/// Represents an item in a queue.
#[async_trait]
pub trait QueueItem
where
    Self: Send + Sync + 'static,
{
    /// Returns the length of the item in seconds, if known.
    fn length(&self) -> Option<f32>;

    /// Registers a sink with the item.
    fn register_sink(&self, sink_id: SinkId);

    /// Returns the sink id of the item, if registered.
    /// If this returns `None`, the loadable() method will be called to create a new sink.
    fn sink_id(&self) -> Option<SinkId>;

    /// Returns an id that is used to identify the item for external users of the api
    fn item_id(&self) -> String;

    /// Returns the item's loadable.
    /// This is async because some sources may need to do additional work to create a loadable.
    async fn loadable(&self) -> BoxedLoadable;
}

/// [QueueItem] trait object.
pub struct BoxedQueueItem(Box<dyn QueueItem>);

#[async_trait]
impl QueueItem for BoxedQueueItem {
    fn length(&self) -> Option<f32> {
        self.0.length()
    }

    fn register_sink(&self, sink_id: SinkId) {
        self.0.register_sink(sink_id)
    }

    fn sink_id(&self) -> Option<SinkId> {
        self.0.sink_id()
    }

    fn item_id(&self) -> String {
        self.0.item_id()
    }

    async fn loadable(&self) -> BoxedLoadable {
        self.0.loadable().await
    }
}
