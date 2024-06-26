/// The type used for primary keys in the database.
pub type PrimaryKey = u32;

/// A turntable account
#[derive(Debug, Clone)]
pub struct UserData {
    pub id: PrimaryKey,
    pub username: String,
    pub password: String,
    pub display_name: String,
    pub superuser: bool,
}

/// Login session data for authentication
#[derive(Debug, Clone)]
pub struct SessionData {
    pub id: PrimaryKey,
    /// The session token, or key if you will
    pub token: String,
    /// The user that is logged in
    pub user: UserData,
}

/// A turntable room
#[derive(Debug, Clone)]
pub struct RoomData {
    pub id: PrimaryKey,
    /// A slug used to identify the room
    pub slug: String,
    pub title: String,
    pub description: String,
    pub members: Vec<RoomMemberData>,
}

/// A member of a room
#[derive(Debug, Clone)]
pub struct RoomMemberData {
    pub id: PrimaryKey,
    /// If this is true, the member has full control over the room
    pub owner: bool,
    pub user: UserData,
}

/// An invitation to a room and account creation
#[derive(Debug, Clone)]
pub struct RoomInviteData {
    pub id: PrimaryKey,
    /// The unique token identifier of the invite
    pub token: String,
    pub room: RoomData,
    pub inviter: UserData,
}

/// A stream key is used to access the audio stream of a room
/// Note: `source`, `room_id`, and `user_id` are unique together.
#[derive(Debug, Clone)]
pub struct StreamKeyData {
    pub id: PrimaryKey,
    /// The unique token used to identify the stream key
    pub token: String,
    /// What app or source this stream key is being used from.
    /// Example: VLC, turntable, etc
    pub source: String,
    /// The room this stream key
    pub room_id: PrimaryKey,
    /// The user this stream key belongs to
    pub user_id: PrimaryKey,
}
