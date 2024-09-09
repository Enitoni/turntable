use std::sync::Arc;

use log::info;
use parking_lot::Mutex;
use turntable_core::PlayerContext as Player;
use turntable_impls::WaveEncoder;

use crate::{
    events::CollabEvent, CollabContext, LinearQueue, LinearQueueItem, PrimaryKey, RoomData,
    RoomMemberData, WrappedQueueNotifier,
};

use super::{RoomConnection, RoomConnectionHandle, RoomConnectionId, RoomError};

pub type RoomId = PrimaryKey;

/// A turntable room, containing listeners, a queue, and a player.
pub struct Room {
    context: CollabContext,
    state: Mutex<RoomState>,
    data: Mutex<RoomData>,
    /// The users currently connected and listening in this room
    connections: Mutex<Vec<RoomConnection>>,
}

#[derive(Default)]
pub enum RoomState {
    #[default]
    Inactive,
    Active {
        player: Arc<Player>,
        queue: Arc<LinearQueue>,
    },
}

impl Room {
    pub fn new(context: &CollabContext, data: RoomData) -> Self {
        Self {
            context: context.clone(),
            state: Default::default(),
            connections: Default::default(),
            data: data.into(),
        }
    }

    /// Activates the room, which means it has an active player and queue associated.
    pub fn activate(&self) {
        info!("Activating room...");
        let new_player = self.context.pipeline.create_player();

        let new_queue = self
            .context
            .pipeline
            .create_queue(new_player.id, |notifier| {
                LinearQueue::new(WrappedQueueNotifier {
                    room_id: self.id(),
                    context: self.context.clone(),
                    notifier,
                })
            });

        info!("Room {} activated", self.data().title);

        *self.state.lock() = RoomState::Active {
            player: new_player.into(),
            queue: new_queue,
        }
    }

    /// Ensure the room is activated
    fn ensure_activation(&self) {
        let is_inactive = {
            let state = self.state.lock();
            matches!(*state, RoomState::Inactive)
        };

        if is_inactive {
            self.activate()
        }
    }

    /// Returns the currently playing queue item, if any
    pub fn current_item(&self) -> Option<LinearQueueItem> {
        let state = self.state.lock();

        match &*state {
            RoomState::Inactive => None,
            RoomState::Active { player, queue } => player
                .current_sink()
                .and_then(|id| queue.get_by_sink_id(id)),
        }
    }

    /// Gets the associated queue if the room is active
    pub fn queue(&self) -> Result<Arc<LinearQueue>, RoomError> {
        // Activate room if queue is accessed
        self.ensure_activation();

        let state = self.state.lock();

        match &*state {
            RoomState::Inactive => Err(RoomError::RoomNotActive),
            RoomState::Active { player: _, queue } => Ok(queue.clone()),
        }
    }

    /// Gets the player if the room is active
    pub fn player(&self) -> Result<Arc<Player>, RoomError> {
        let state = self.state.lock();

        match &*state {
            RoomState::Inactive => Err(RoomError::RoomNotActive),
            RoomState::Active { player, queue: _ } => Ok(player.clone()),
        }
    }

    /// Registers an added member to the room
    pub fn add_member(&self, new_member: RoomMemberData) {
        self.data.lock().members.push(new_member.clone());

        self.context.emit(CollabEvent::UserJoined {
            room_id: self.id(),
            new_member,
        });
    }

    /// Returns the member if it exists in the room
    pub fn member_by_user_id(&self, user_id: PrimaryKey) -> Result<RoomMemberData, RoomError> {
        self.data
            .lock()
            .members
            .iter()
            .find(|m| m.user.id == user_id)
            .cloned()
            .ok_or(RoomError::UserNotInRoom)
    }

    /// Creates a stream connection to the room.
    pub fn connect(
        &self,
        user_id: PrimaryKey,
        source: String,
        with_latency: Option<u32>,
    ) -> Result<RoomConnectionHandle, RoomError> {
        // Ensure the user is actually in the room before doing anything else
        let member = self.member_by_user_id(user_id)?;

        // For now, just activate a room if it's not active when a user wants to connect
        self.ensure_activation();

        let player = self.player()?;
        let stream = self
            .context
            .pipeline
            .consume_player::<WaveEncoder>(player.id, with_latency);

        let connection = RoomConnection::new(user_id, stream.id, source.clone());
        let connection_id = connection.id;

        self.connections.lock().push(connection);

        info!(
            "User {} connected to room {} via {}",
            member.user.display_name,
            self.data().title,
            source
        );

        self.context.emit(CollabEvent::UserConnected {
            room_id: self.id(),
            user_id,
            source,
        });

        Ok(RoomConnectionHandle::new(
            &self.context,
            connection_id,
            self.id(),
            stream,
        ))
    }

    /// Called when a [RoomConnectionHandle] is dropped
    pub fn remove_connection(&self, connection_id: RoomConnectionId) {
        let mut connections = self.connections.lock();

        let connection = connections
            .iter()
            .find(|c| c.id == connection_id)
            .expect("connection exists when trying to remove it");

        let member = self
            .member_by_user_id(connection.user_id)
            .ok()
            .map(|m| m.user.display_name.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        info!(
            "User {} disconnected from room {} via {}",
            member,
            self.data().title,
            connection.source
        );

        self.context.emit(CollabEvent::UserDisconnected {
            room_id: self.id(),
            user_id: connection.user_id,
            source: connection.source.to_owned(),
        });

        connections.retain(|c| c.id != connection_id)
    }

    /// Returns the current connections. This can be the same member multiple times.
    pub fn current_connections(&self) -> Vec<RoomConnection> {
        self.connections.lock().clone()
    }

    pub fn data(&self) -> RoomData {
        self.data.lock().clone()
    }

    pub fn id(&self) -> RoomId {
        self.data().id
    }
}
