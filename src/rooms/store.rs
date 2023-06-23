use std::sync::Weak;

use crate::{store::Store, EventEmitter};

#[derive(Debug)]
pub struct RoomStore {
    store: Weak<Store>,
    emitter: EventEmitter,
}

impl RoomStore {
    pub fn new(store: Weak<Store>, emitter: EventEmitter) -> Self {
        Self { store, emitter }
    }
}
