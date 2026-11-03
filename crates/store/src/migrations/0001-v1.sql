-- The tables of YouCraft's first version. Identifiers are the 64 bits of youcraft-api's ids as
-- INTEGER; times are seconds since 1970 (UTC); media time is whole ticks of a timescale.

-- People who signed in with Google, and what they are to the site.
CREATE TABLE accounts (
    id INTEGER PRIMARY KEY,
    google_subject TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL,
    name TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'viewer' CHECK (role IN ('viewer', 'creator', 'admin')),
    blocked INTEGER NOT NULL DEFAULT 0 CHECK (blocked IN (0, 1)),
    language TEXT,
    theme TEXT NOT NULL DEFAULT 'device' CHECK (theme IN ('device', 'dark', 'light')),
    history_paused INTEGER NOT NULL DEFAULT 0 CHECK (history_paused IN (0, 1)),
    created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX accounts_by_email ON accounts (email COLLATE NOCASE);

-- E-mail addresses invited to publish or to run the site; applied when that person signs in.
CREATE TABLE invitations (
    email TEXT PRIMARY KEY COLLATE NOCASE,
    role TEXT NOT NULL CHECK (role IN ('creator', 'admin')),
    created_at INTEGER NOT NULL
) STRICT;

-- Signed-in browsers, by the SHA-256 of the secret their cookie holds.
CREATE TABLE sessions (
    secret_hash BLOB PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
) STRICT;
CREATE INDEX sessions_of_account ON sessions (account_id);
CREATE INDEX sessions_expiring ON sessions (expires_at);

-- Personal tokens of programs, by the SHA-256 of their secret.
CREATE TABLE tokens (
    id INTEGER PRIMARY KEY,
    secret_hash BLOB NOT NULL UNIQUE,
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER
) STRICT;
CREATE INDEX tokens_of_account ON tokens (account_id, created_at DESC);

-- Every account has one channel, its public face; `handle_key` is the handle case-folded.
CREATE TABLE channels (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL UNIQUE REFERENCES accounts (id) ON DELETE CASCADE,
    handle TEXT NOT NULL,
    handle_key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    subscriber_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE videos (
    id INTEGER PRIMARY KEY,
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    visibility TEXT NOT NULL DEFAULT 'private' CHECK (visibility IN ('public', 'unlisted', 'private')),
    state TEXT NOT NULL DEFAULT 'uploading' CHECK (state IN ('uploading', 'processing', 'ready', 'failed')),
    failure TEXT,
    duration_ticks INTEGER NOT NULL DEFAULT 0,
    timescale INTEGER NOT NULL DEFAULT 1000 CHECK (timescale > 0),
    width INTEGER NOT NULL DEFAULT 0,
    height INTEGER NOT NULL DEFAULT 0,
    comments_enabled INTEGER NOT NULL DEFAULT 1 CHECK (comments_enabled IN (0, 1)),
    view_count INTEGER NOT NULL DEFAULT 0,
    like_count INTEGER NOT NULL DEFAULT 0,
    dislike_count INTEGER NOT NULL DEFAULT 0,
    comment_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    published_at INTEGER
) STRICT;
CREATE INDEX videos_of_channel ON videos (channel_id, created_at DESC);
CREATE INDEX videos_watchable ON videos (published_at DESC) WHERE visibility = 'public' AND state = 'ready';

-- One row per rendition a video was made into.
CREATE TABLE renditions (
    video_id INTEGER NOT NULL REFERENCES videos (id) ON DELETE CASCADE,
    height INTEGER NOT NULL,
    width INTEGER NOT NULL,
    bandwidth INTEGER NOT NULL,
    codecs TEXT NOT NULL,
    PRIMARY KEY (video_id, height)
) STRICT;

-- Uploads in progress, received from `offset` 0 to `received`, and the work to do on them.
CREATE TABLE uploads (
    id INTEGER PRIMARY KEY,
    video_id INTEGER NOT NULL UNIQUE REFERENCES videos (id) ON DELETE CASCADE,
    size INTEGER NOT NULL,
    received INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE jobs (
    id INTEGER PRIMARY KEY,
    video_id INTEGER NOT NULL REFERENCES videos (id) ON DELETE CASCADE,
    state TEXT NOT NULL DEFAULT 'queued' CHECK (state IN ('queued', 'running', 'done', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    progress_permille INTEGER NOT NULL DEFAULT 0,
    failure TEXT,
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    finished_at INTEGER
) STRICT;
CREATE INDEX jobs_waiting ON jobs (created_at) WHERE state = 'queued';

-- +1 a like, -1 a dislike.
CREATE TABLE likes (
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    video_id INTEGER NOT NULL REFERENCES videos (id) ON DELETE CASCADE,
    value INTEGER NOT NULL CHECK (value IN (-1, 1)),
    created_at INTEGER NOT NULL,
    PRIMARY KEY (account_id, video_id)
) STRICT;
CREATE INDEX likes_of_video ON likes (video_id);

CREATE TABLE subscriptions (
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (account_id, channel_id)
) STRICT;
CREATE INDEX subscribers_of_channel ON subscriptions (channel_id);

-- Comments, and replies to them (`parent_id`, one level deep, as on YouTube).
CREATE TABLE comments (
    id INTEGER PRIMARY KEY,
    video_id INTEGER NOT NULL REFERENCES videos (id) ON DELETE CASCADE,
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    parent_id INTEGER REFERENCES comments (id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    like_count INTEGER NOT NULL DEFAULT 0,
    reply_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    edited_at INTEGER
) STRICT;
CREATE INDEX comments_of_video ON comments (video_id, parent_id, created_at);

CREATE TABLE comment_likes (
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    comment_id INTEGER NOT NULL REFERENCES comments (id) ON DELETE CASCADE,
    PRIMARY KEY (account_id, comment_id)
) STRICT;

-- Accounts a channel stopped from commenting on its videos.
CREATE TABLE blocks (
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (channel_id, account_id)
) STRICT;

-- Playlists; each account has at most one 'watch_later', its Watch later list.
CREATE TABLE playlists (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    kind TEXT NOT NULL DEFAULT 'playlist' CHECK (kind IN ('playlist', 'watch_later')),
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    visibility TEXT NOT NULL DEFAULT 'private' CHECK (visibility IN ('public', 'unlisted', 'private')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX one_watch_later ON playlists (account_id) WHERE kind = 'watch_later';

CREATE TABLE playlist_items (
    playlist_id INTEGER NOT NULL REFERENCES playlists (id) ON DELETE CASCADE,
    video_id INTEGER NOT NULL REFERENCES videos (id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (playlist_id, video_id)
) STRICT;
CREATE INDEX playlist_order ON playlist_items (playlist_id, position);

-- What each person watched, and where they stopped, in milliseconds.
CREATE TABLE history (
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    video_id INTEGER NOT NULL REFERENCES videos (id) ON DELETE CASCADE,
    watched_at INTEGER NOT NULL,
    position_ms INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (account_id, video_id)
) STRICT;
CREATE INDEX history_recent ON history (account_id, watched_at DESC);

-- One view per viewer, video and day; `viewer_hash` never names the viewer.
CREATE TABLE views (
    video_id INTEGER NOT NULL REFERENCES videos (id) ON DELETE CASCADE,
    viewer_hash BLOB NOT NULL,
    day INTEGER NOT NULL,
    PRIMARY KEY (video_id, viewer_hash, day)
) STRICT, WITHOUT ROWID;

-- Full-text search over titles, descriptions and channel names, in any script; rowid is the video.
CREATE VIRTUAL TABLE video_search USING fts5 (
    title, description, channel,
    content = '', contentless_delete = 1,
    tokenize = 'trigram remove_diacritics 1'
);
