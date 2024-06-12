use async_trait::async_trait;

use crate::BoxedLoadable;

/// Represents an item in a queue.
#[async_trait]
pub trait QueueItem
where
    Self: Send + Sync + 'static,
{
    /// Returns the length of the item in seconds, if known.
    fn length(&self) -> Option<f32>;

    /// Returns the id of the item.
    fn id(&self) -> String;

    /// Returns the item's loadable.
    /// This is async because some sources may need to do additional work to create a loadable.
    async fn loadable(&self) -> BoxedLoadable;
}

/// [QueueItem] trait object.
pub type BoxedQueueItem = Box<dyn QueueItem>;

#[async_trait]
impl QueueItem for BoxedQueueItem {
    fn length(&self) -> Option<f32> {
        self.as_ref().length()
    }

    fn id(&self) -> String {
        self.as_ref().id()
    }

    async fn loadable(&self) -> BoxedLoadable {
        self.loadable().await
    }
}
