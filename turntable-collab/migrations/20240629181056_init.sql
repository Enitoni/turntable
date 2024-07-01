-- Add migration script here

CREATE TABLE users (
  id SERIAL PRIMARY KEY,
  username TEXT NOT NULL UNIQUE,
  password TEXT NOT NULL,
  display_name TEXT NOT NULL,
  superuser BOOLEAN NOT NULL DEFAULT false
);

CREATE TABLE sessions (
  id SERIAL PRIMARY KEY,
  token TEXT NOT NULL UNIQUE,
  user_id INT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  expires_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE rooms (
  id SERIAL PRIMARY KEY,
  slug TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL,
  description TEXT
);

CREATE TABLE room_members (
  id SERIAL PRIMARY KEY,
  user_id INT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  room_id INT NOT NULL REFERENCES rooms (id) ON DELETE CASCADE,
  owner BOOLEAN NOT NULL DEFAULT false
);

CREATE UNIQUE INDEX room_member_unique ON room_members (user_id, room_id);

CREATE TABLE room_invites (
  id SERIAL PRIMARY KEY,
  token TEXT NOT NULL UNIQUE,
  room_id INT NOT NULL REFERENCES rooms (id) ON DELETE CASCADE,
  inviter_id INT NOT NULL REFERENCES users (id) ON DELETE CASCADE
);

CREATE TABLE stream_keys (
  id SERIAL PRIMARY KEY,
  token TEXT NOT NULL UNIQUE,
  source TEXT NOT NULL,
  room_id INT NOT NULL REFERENCES rooms (id) ON DELETE CASCADE,
  user_id INT NOT NULL REFERENCES users (id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX stream_key_unique ON stream_keys (source, room_id, user_id);