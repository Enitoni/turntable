mod buffer;
mod ext;
mod id;
mod introspection;

pub use buffer::*;
pub use ext::*;
pub use id::*;
pub use introspection::*;

use tokio::runtime::{Handle, Runtime};

// idk where else to put this lol
/// Returns the current tokio handle, or creates a new one if none exists.
pub fn get_or_create_handle() -> Handle {
    Handle::try_current()
        .ok()
        .unwrap_or_else(|| Runtime::new().unwrap().handle().clone())
}
