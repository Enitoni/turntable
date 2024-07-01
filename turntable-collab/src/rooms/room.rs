use std::sync::Arc;

use parking_lot::Mutex;
use turntable_core::{Ingestion, PlayerContext as Player};
use turntable_impls::WaveEncoder;

use crate::{CollabContext, Database, LinearQueue, PrimaryKey, RoomData, RoomMemberData, Track};

use super::{RoomConnection, RoomConnectionHandle, RoomConnectionId, RoomError};

pub type RoomId = PrimaryKey;

/// A turntable room, containing listeners, a queue, and a player.
pub struct Room<I, Db> {
    context: CollabContext<I, Db>,
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

impl<I, Db> Room<I, Db>
where
    I: Ingestion,
    Db: Database,
{
    pub fn new(context: &CollabContext<I, Db>, data: RoomData) -> Self {
        Self {
            context: context.clone(),
            state: Default::default(),
            connections: Default::default(),
            data: data.into(),
        }
    }

    /// Activates the room, which means it has an active player and queue associated.
    pub fn activate(&self) {
        let new_player = self.context.pipeline.create_player();
        let new_queue = self
            .context
            .pipeline
            .create_queue::<LinearQueue>(new_player.id);

        *self.state.lock() = RoomState::Active {
            player: new_player.into(),
            queue: new_queue,
        }
    }

    /// Returns the currently playing track, if any
    pub fn current_track(&self) -> Option<Track> {
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
        self.data.lock().members.push(new_member)
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
    ) -> Result<RoomConnectionHandle<I, Db>, RoomError> {
        // Ensure the user is actually in the room before doing anything else
        let _ = self.member_by_user_id(user_id)?;

        let connection = RoomConnection::new(user_id, source);
        let connection_id = connection.id;

        self.connections.lock().push(connection);

        let player = self.player()?;
        let stream = self
            .context
            .pipeline
            .consume_player::<WaveEncoder>(player.id);

        Ok(RoomConnectionHandle::new(
            &self.context,
            connection_id,
            self.id(),
            stream,
        ))
    }

    /// Called when a [RoomConnectionHandle] is dropped
    pub fn remove_connection(&self, connection_id: RoomConnectionId) {
        self.connections.lock().retain(|c| c.id != connection_id)
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