use async_trait::async_trait;
use sqlx::{postgres::PgPoolOptions, query, query_as, Error as SqlxError, PgPool};

use crate::{
    Database, DatabaseError, DatabaseResult, IntoDatabaseError, NewRoom, NewRoomInvite,
    NewRoomMember, NewSession, NewStreamKey, NewUser, PrimaryKey, Result, RoomData, RoomInviteData,
    RoomMemberData, SessionData, StreamKeyData, UpdatedRoom, UpdatedUser, UserData,
};

/// A postgres database implementation for turntable
pub struct PgDatabase {
    pool: PgPool,
}

impl PgDatabase {
    pub async fn new(url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await
            .map_err(|e| DatabaseError::Internal(Box::new(e)))?;

        Ok(Self { pool })
    }

    async fn room_members(&self, room_id: PrimaryKey) -> Result<Vec<RoomMemberData>> {
        let member_rows = query!(
            "
            SELECT
                room_members.*,
                users.username,
                users.password,
                users.display_name,
                users.superuser
            FROM room_members
                INNER JOIN users ON room_members.user_id = users.id
            WHERE room_id = $1",
            room_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.any())?;

        let members: Vec<_> = member_rows
            .into_iter()
            .map(|r| RoomMemberData {
                id: r.id,
                owner: r.owner,
                user: UserData {
                    id: r.user_id,
                    username: r.username,
                    password: r.password,
                    display_name: r.display_name,
                    superuser: r.superuser,
                },
            })
            .collect();

        Ok(members)
    }
}

#[async_trait]
impl Database for PgDatabase {
    async fn check_for_superuser(&self) -> Result<bool> {
        let result = query!("SELECT superuser FROM users WHERE superuser = true")
            .fetch_one(&self.pool)
            .await;

        match result {
            Ok(_) => Ok(true),
            Err(e) => match e {
                SqlxError::RowNotFound => Ok(false),
                e => Err(e.any()),
            },
        }
    }

    async fn user_by_id(&self, user_id: PrimaryKey) -> Result<UserData> {
        query_as!(UserData, "SELECT * FROM users WHERE id = $1", user_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.not_found_or("user", "id"))
    }

    async fn user_by_username(&self, username: &str) -> Result<UserData> {
        query_as!(
            UserData,
            "SELECT * FROM users WHERE username = $1",
            username
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.not_found_or("user", "username"))
    }

    async fn create_user(&self, new_user: NewUser) -> Result<UserData> {
        self.user_by_username(&new_user.username)
            .await
            .conflict_or_ok("user", "username", &new_user.username)?;

        query_as!(UserData, "INSERT INTO users (username, password, display_name, superuser) VALUES ($1, $2, $3, $4) RETURNING *",
            new_user.username,
            new_user.password,
            new_user.display_name,
            new_user.superuser
        ).fetch_one(&self.pool).await.map_err(|e| e.any())
    }

    async fn update_user(&self, updated_user: UpdatedUser) -> Result<UserData> {
        let user = self.user_by_id(updated_user.id).await?;

        query!(
            "UPDATE users SET display_name = $1 WHERE id = $2",
            updated_user.display_name.unwrap_or(user.display_name),
            updated_user.id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| e.any())?;

        self.user_by_id(updated_user.id).await
    }

    async fn delete_user(&self, user_id: PrimaryKey) -> Result<()> {
        // Ensure user exists
        let _ = self.user_by_id(user_id).await?;

        query!("DELETE FROM users WHERE id = $1", user_id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.any())
            .map(|_| ())
    }

    async fn session_by_token(&self, token: &str) -> Result<SessionData> {
        let row = query!(
            "SELECT
                sessions.*,
                users.username,
                users.password,
                users.display_name,
                users.superuser
            FROM sessions
                INNER JOIN users ON sessions.user_id = users.id
             WHERE token = $1
            ",
            token
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.not_found_or("session", "token"))?;

        let result = SessionData {
            id: row.id,
            token: row.token,
            expires_at: row.expires_at,
            user: UserData {
                id: row.user_id,
                username: row.username,
                password: row.password,
                display_name: row.display_name,
                superuser: row.superuser,
            },
        };

        Ok(result)
    }

    async fn create_session(&self, new_session: NewSession) -> Result<SessionData> {
        self.session_by_token(&new_session.token)
            .await
            .conflict_or_ok("session", "token", &new_session.token)?;

        let record = query!(
            "INSERT INTO sessions (token, user_id, expires_at) VALUES ($1, $2, $3) RETURNING token",
            new_session.token,
            new_session.user_id,
            new_session.expires_at
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.any())?;

        self.session_by_token(&record.token).await
    }

    async fn delete_session_by_token(&self, token: &str) -> Result<()> {
        // Ensure session exists
        let _ = self.session_by_token(token).await?;

        query!("DELETE FROM sessions WHERE token = $1", token)
            .execute(&self.pool)
            .await
            .map_err(|e| e.any())
            .map(|_| ())
    }

    async fn clear_expired_sessions(&self) -> Result<()> {
        query!("DELETE FROM sessions WHERE timezone('UTC', now()) > expires_at")
            .execute(&self.pool)
            .await
            .map_err(|e| e.any())
            .map(|_| ())
    }

    async fn room_by_id(&self, room_id: PrimaryKey) -> Result<RoomData> {
        let room_row = query!("SELECT * FROM rooms WHERE id = $1", room_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.not_found_or("room", "id"))?;

        let members = self.room_members(room_id).await?;

        Ok(RoomData {
            id: room_row.id,
            slug: room_row.slug,
            title: room_row.title,
            description: room_row.description,
            members,
        })
    }

    async fn room_by_slug(&self, slug: &str) -> Result<RoomData> {
        let row = query!("SELECT id FROM rooms WHERE slug = $1", slug)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.not_found_or("room", "slug"))?;

        self.room_by_id(row.id).await
    }

    async fn room_invite_by_token(&self, token: &str) -> Result<RoomInviteData> {
        let row = query!(
            "SELECT
                invites.*,
                users.username,
                users.password,
                users.display_name,
                users.superuser,
                rooms.slug,
                rooms.title,
                rooms.description
            FROM room_invites AS invites
                INNER JOIN users ON invites.inviter_id = users.id
                INNER JOIN rooms ON invites.room_id = rooms.id
            WHERE token = $1
            ",
            token
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.not_found_or("room invite", "token"))?;

        let members = self.room_members(row.room_id).await?;

        Ok(RoomInviteData {
            id: row.id,
            token: row.token,
            room: RoomData {
                id: row.room_id,
                slug: row.slug,
                title: row.title,
                description: row.description,
                members,
            },
            inviter: UserData {
                id: row.inviter_id,
                username: row.username,
                password: row.password,
                display_name: row.display_name,
                superuser: row.superuser,
            },
        })
    }

    async fn list_rooms(&self) -> Result<Vec<RoomData>> {
        let mut rooms: Vec<_> = query!("SELECT * FROM rooms")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.any())?
            .into_iter()
            .map(|row| RoomData {
                id: row.id,
                slug: row.slug,
                title: row.title,
                description: row.description,
                members: vec![],
            })
            .collect();

        for room in rooms.iter_mut() {
            room.members = self.room_members(room.id).await?
        }

        Ok(rooms)
    }

    async fn create_room(&self, new_room: NewRoom) -> Result<RoomData> {
        self.room_by_slug(&new_room.slug)
            .await
            .conflict_or_ok("room", "slug", &new_room.slug)?;

        let user = self.user_by_id(new_room.user_id).await?;
        let room = query!(
            "
            INSERT INTO rooms (slug, title, description)
            VALUES ($1, $2, $3)
            RETURNING id
        ",
            new_room.slug,
            new_room.title,
            new_room.description
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.any())?;

        // Add owner as a member to the room
        self.create_room_member(NewRoomMember {
            user_id: user.id,
            room_id: room.id,
            owner: true,
        })
        .await?;

        self.room_by_id(room.id).await
    }

    async fn create_room_member(&self, new_member: NewRoomMember) -> Result<()> {
        // Ensure the user isn't a member of this room already
        query!(
            "SELECT id FROM room_members WHERE user_id = $1 AND room_id = $2",
            new_member.user_id,
            new_member.room_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.not_found_or("", ""))
        .conflict_or_ok(
            "room member",
            "user:room",
            format!("{}:{}", new_member.user_id, new_member.room_id).as_str(),
        )?;

        query!(
            "
            INSERT INTO room_members (user_id, room_id, owner)
            VALUES ($1, $2, $3)",
            new_member.user_id,
            new_member.room_id,
            new_member.owner,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| e.any())
        .map(|_| ())
    }

    async fn update_room(&self, updated_room: UpdatedRoom) -> Result<RoomData> {
        let room = self.room_by_id(updated_room.id).await?;

        query!(
            "UPDATE rooms SET
                title = $1,
                description = $2
            WHERE id = $3",
            updated_room.title.unwrap_or(room.title),
            updated_room.description.or(room.description),
            updated_room.id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| e.any())?;

        self.room_by_id(updated_room.id).await
    }

    async fn delete_room(&self, room_id: PrimaryKey) -> Result<()> {
        // Ensure room exists
        let _ = self.room_by_id(room_id).await?;

        query!("DELETE FROM rooms WHERE id = $1", room_id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.any())
            .map(|_| ())
    }

    async fn delete_room_member(&self, room_id: PrimaryKey, user_id: PrimaryKey) -> Result<()> {
        let member = query!(
            "SELECT id FROM room_members WHERE room_id = $1 AND user_id = $2",
            room_id,
            user_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.not_found_or("room member", "room_id:user_id"))?;

        query!("DELETE FROM room_members WHERE id = $1", member.id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.any())
            .map(|_| ())
    }

    async fn create_room_invite(&self, new_room_invite: NewRoomInvite) -> Result<RoomInviteData> {
        self.room_invite_by_token(&new_room_invite.token)
            .await
            .conflict_or_ok("room invite", "token", &new_room_invite.token)?;

        let invite = query!(
            "INSERT INTO room_invites (token, room_id, inviter_id) VALUES ($1, $2, $3) RETURNING token",
            new_room_invite.token,
            new_room_invite.room_id,
            new_room_invite.user_id
        ).fetch_one(&self.pool).await.map_err(|e| e.any())?;

        self.room_invite_by_token(&invite.token).await
    }

    async fn delete_room_invite(&self, invite_id: PrimaryKey) -> Result<()> {
        // Ensure invite exists
        query!("SELECT id FROM room_invites WHERE id = $1", invite_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.not_found_or("room invite", "id"))?;

        query!("DELETE FROM room_invites WHERE id = $1", invite_id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.any())
            .map(|_| ())
    }

    async fn stream_key_by_token(&self, token: &str) -> Result<StreamKeyData> {
        query_as!(
            StreamKeyData,
            "SELECT * FROM stream_keys WHERE token = $1",
            token
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.not_found_or("stream key", "token"))
    }

    async fn create_stream_key(&self, new_key: NewStreamKey) -> Result<StreamKeyData> {
        query!(
            "
            SELECT id FROM stream_keys
            WHERE token = $1 OR (room_id = $2 AND user_id = $3)",
            new_key.token,
            new_key.room_id,
            new_key.user_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.not_found_or("", ""))
        .conflict_or_ok(
            "stream key",
            "token or room:user",
            format!(
                "{} or {}:{}",
                &new_key.token, new_key.room_id, new_key.user_id
            )
            .as_str(),
        )?;

        todo!()
    }

    async fn list_stream_keys(
        &self,
        room_id: PrimaryKey,
        user_id: PrimaryKey,
    ) -> Result<Vec<StreamKeyData>> {
        query_as!(
            StreamKeyData,
            "SELECT * FROM stream_keys WHERE room_id = $1 AND user_id = $2",
            room_id,
            user_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.any())
    }

    async fn delete_stream_key(&self, key_id: PrimaryKey) -> Result<()> {
        // Ensure key exists
        query!("SELECT id FROM stream_keys WHERE id = $1", key_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.not_found_or("stream key", "id"))?;

        query!("DELETE FROM stream_keys WHERE id = $1", key_id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.any())
            .map(|_| ())
    }
}

impl IntoDatabaseError for SqlxError {
    fn any(self) -> DatabaseError {
        DatabaseError::Internal(Box::new(self))
    }

    fn not_found_or(self, resource: &'static str, identifier: &'static str) -> DatabaseError {
        match self {
            SqlxError::RowNotFound => DatabaseError::NotFound {
                resource,
                identifier,
            },
            e => Self::any(e),
        }
    }
}
