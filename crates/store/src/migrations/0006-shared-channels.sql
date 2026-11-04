-- Channels that several people run. An account is a person who signs in; a channel is a public
-- identity, run by its members, each with a role: viewer < editor < manager < owner. Everything a
-- person does on the site they do as a channel: it comments, likes, subscribes, keeps playlists
-- and a history. A comment also keeps the account that wrote it, which nobody sees, so that a
-- person the site blocks hides whatever they wrote as any channel. Each account owned the one
-- channel it had; it is that channel's owner from now on.
--
-- Every trigger goes first and comes back last, since the tables they read are rebuilt in
-- between: rebuilt as SQLite asks, a new table made beside the old, filled, the old dropped and the
-- new renamed, with foreign keys off (see migrations.rs).

DROP TRIGGER rating_given;
DROP TRIGGER rating_changed;
DROP TRIGGER rating_taken_back;
DROP TRIGGER subscribed;
DROP TRIGGER unsubscribed;
DROP TRIGGER commented;
DROP TRIGGER uncommented;
DROP TRIGGER comment_hidden;
DROP TRIGGER comment_liked;
DROP TRIGGER comment_unliked;
DROP TRIGGER viewed;
DROP TRIGGER video_indexed;
DROP TRIGGER video_reindexed;
DROP TRIGGER video_unindexed;
DROP TRIGGER channel_renamed;
DROP TRIGGER channel_blocked;
DROP TRIGGER channel_unblocked;
DROP TRIGGER account_blocked;
DROP TRIGGER stored_bytes_changed;

-- Who runs each channel, and as what.
CREATE TABLE members (
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('viewer', 'editor', 'manager', 'owner')),
    created_at INTEGER NOT NULL,
    PRIMARY KEY (channel_id, account_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX channels_of_account ON members (account_id, created_at);
INSERT INTO members (channel_id, account_id, role, created_at)
SELECT id, account_id, 'owner', created_at FROM channels;

-- E-mail addresses a channel's owners invited to run it, until the person with that address says
-- yes or no, or `expires_at` passes.
CREATE TABLE channel_invitations (
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    email TEXT NOT NULL COLLATE NOCASE,
    role TEXT NOT NULL CHECK (role IN ('viewer', 'editor', 'manager', 'owner')),
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    PRIMARY KEY (channel_id, email)
) STRICT, WITHOUT ROWID;
CREATE INDEX channel_invitations_of_email ON channel_invitations (email, expires_at);

-- A channel no longer belongs to one account, and keeps whether its history is paused.
CREATE TABLE channels_new (
    id INTEGER PRIMARY KEY,
    handle TEXT NOT NULL,
    handle_key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    subscriber_count INTEGER NOT NULL DEFAULT 0,
    history_paused INTEGER NOT NULL DEFAULT 0 CHECK (history_paused IN (0, 1)),
    photo TEXT,
    photo_bytes INTEGER NOT NULL DEFAULT 0 CHECK (photo_bytes >= 0),
    created_at INTEGER NOT NULL
) STRICT;
INSERT INTO channels_new (id, handle, handle_key, name, description, subscriber_count,
    history_paused, photo, photo_bytes, created_at)
SELECT c.id, c.handle, c.handle_key, c.name, c.description, c.subscriber_count,
    a.history_paused, c.photo, c.photo_bytes, c.created_at
FROM channels c JOIN accounts a ON a.id = c.account_id;

-- What each account did, it did as its channel: `channel_of` names it while the rows move.
CREATE TABLE channel_of (account_id INTEGER PRIMARY KEY, channel_id INTEGER NOT NULL) STRICT;
INSERT INTO channel_of SELECT account_id, id FROM channels;

CREATE TABLE likes_new (
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    video_id INTEGER NOT NULL REFERENCES videos (id) ON DELETE CASCADE,
    value INTEGER NOT NULL CHECK (value IN (-1, 1)),
    created_at INTEGER NOT NULL,
    PRIMARY KEY (channel_id, video_id)
) STRICT;
INSERT INTO likes_new SELECT o.channel_id, l.video_id, l.value, l.created_at
FROM likes l JOIN channel_of o ON o.account_id = l.account_id;

CREATE TABLE subscriptions_new (
    subscriber_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (subscriber_id, channel_id)
) STRICT;
INSERT INTO subscriptions_new SELECT o.channel_id, s.channel_id, s.created_at
FROM subscriptions s JOIN channel_of o ON o.account_id = s.account_id;

-- A comment's author is a channel; `account_id` is the person who wrote it as that channel.
CREATE TABLE comments_new (
    id INTEGER PRIMARY KEY,
    video_id INTEGER NOT NULL REFERENCES videos (id) ON DELETE CASCADE,
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    parent_id INTEGER REFERENCES comments (id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    like_count INTEGER NOT NULL DEFAULT 0,
    reply_count INTEGER NOT NULL DEFAULT 0,
    hidden INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
    created_at INTEGER NOT NULL,
    edited_at INTEGER
) STRICT;
INSERT INTO comments_new (id, video_id, channel_id, account_id, parent_id, body, like_count,
    reply_count, hidden, created_at, edited_at)
SELECT m.id, m.video_id, o.channel_id, m.account_id, m.parent_id, m.body, m.like_count,
    m.reply_count, m.hidden, m.created_at, m.edited_at
FROM comments m JOIN channel_of o ON o.account_id = m.account_id;

CREATE TABLE comment_likes_new (
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    comment_id INTEGER NOT NULL REFERENCES comments (id) ON DELETE CASCADE,
    PRIMARY KEY (channel_id, comment_id)
) STRICT;
INSERT INTO comment_likes_new SELECT o.channel_id, l.comment_id
FROM comment_likes l JOIN channel_of o ON o.account_id = l.account_id;

-- Channels a channel stopped from commenting on its videos.
CREATE TABLE blocks_new (
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    blocked_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (channel_id, blocked_id)
) STRICT;
INSERT INTO blocks_new SELECT b.channel_id, o.channel_id, b.created_at
FROM blocks b JOIN channel_of o ON o.account_id = b.account_id;

CREATE TABLE playlists_new (
    id INTEGER PRIMARY KEY,
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    kind TEXT NOT NULL DEFAULT 'playlist' CHECK (kind IN ('playlist', 'watch_later')),
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    visibility TEXT NOT NULL DEFAULT 'private' CHECK (visibility IN ('public', 'unlisted', 'private')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;
INSERT INTO playlists_new SELECT p.id, o.channel_id, p.kind, p.title, p.description,
    p.visibility, p.created_at, p.updated_at
FROM playlists p JOIN channel_of o ON o.account_id = p.account_id;

CREATE TABLE history_new (
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    video_id INTEGER NOT NULL REFERENCES videos (id) ON DELETE CASCADE,
    watched_at INTEGER NOT NULL,
    position_ms INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (channel_id, video_id)
) STRICT;
INSERT INTO history_new SELECT o.channel_id, h.video_id, h.watched_at, h.position_ms
FROM history h JOIN channel_of o ON o.account_id = h.account_id;

-- An upload belongs to the person sending it, whose share of the site's storage it claims.
CREATE TABLE uploads_new (
    id INTEGER PRIMARY KEY,
    video_id INTEGER NOT NULL UNIQUE REFERENCES videos (id) ON DELETE CASCADE,
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    size INTEGER NOT NULL,
    received INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
) STRICT;
INSERT INTO uploads_new SELECT u.id, u.video_id, c.account_id, u.size, u.received, u.created_at
FROM uploads u JOIN videos v ON v.id = u.video_id JOIN channels c ON c.id = v.channel_id;

DROP TABLE channel_of;
DROP TABLE likes;
ALTER TABLE likes_new RENAME TO likes;
DROP TABLE subscriptions;
ALTER TABLE subscriptions_new RENAME TO subscriptions;
DROP TABLE comment_likes;
ALTER TABLE comment_likes_new RENAME TO comment_likes;
DROP TABLE comments;
ALTER TABLE comments_new RENAME TO comments;
DROP TABLE blocks;
ALTER TABLE blocks_new RENAME TO blocks;
DROP TABLE playlists;
ALTER TABLE playlists_new RENAME TO playlists;
DROP TABLE history;
ALTER TABLE history_new RENAME TO history;
DROP TABLE uploads;
ALTER TABLE uploads_new RENAME TO uploads;
DROP TABLE channels;
ALTER TABLE channels_new RENAME TO channels;
ALTER TABLE accounts DROP COLUMN history_paused;

-- The channel each credential acts as, which its person chose; NULL until they choose, or once
-- the channel is gone, and then the first channel they run (see credentials.rs).
ALTER TABLE sessions ADD COLUMN channel_id INTEGER REFERENCES channels (id) ON DELETE SET NULL;
ALTER TABLE tokens ADD COLUMN channel_id INTEGER REFERENCES channels (id) ON DELETE SET NULL;
CREATE INDEX sessions_of_channel ON sessions (channel_id);
CREATE INDEX tokens_of_channel ON tokens (channel_id);

CREATE INDEX likes_of_video ON likes (video_id);
CREATE INDEX likes_recent ON likes (channel_id, created_at DESC);
CREATE INDEX subscribers_of_channel ON subscriptions (channel_id);
CREATE INDEX subscriptions_recent ON subscriptions (subscriber_id, created_at DESC);
CREATE INDEX comments_of_video ON comments (video_id, parent_id, created_at);
CREATE INDEX comments_liked ON comments (video_id, parent_id, like_count);
CREATE INDEX comments_replies ON comments (parent_id, created_at);
CREATE INDEX comments_of_channel ON comments (channel_id);
CREATE INDEX comments_of_account ON comments (account_id);
CREATE INDEX comment_likes_of_comment ON comment_likes (comment_id);
CREATE INDEX blocks_of_blocked ON blocks (blocked_id);
CREATE INDEX playlists_of_channel ON playlists (channel_id, updated_at DESC);
CREATE UNIQUE INDEX one_watch_later ON playlists (channel_id) WHERE kind = 'watch_later';
CREATE INDEX history_recent ON history (channel_id, watched_at DESC);
CREATE INDEX history_of_video ON history (video_id);
CREATE INDEX uploads_by_age ON uploads (created_at);
CREATE INDEX uploads_of_account ON uploads (account_id);

-- A channel keeps an owner: its last owner may neither leave nor become less, while it stands.
-- A channel deleted takes its members with it, owners included.
CREATE TRIGGER last_owner_stays BEFORE DELETE ON members
WHEN old.role = 'owner'
  AND NOT EXISTS (SELECT 1 FROM members m WHERE m.channel_id = old.channel_id
                  AND m.role = 'owner' AND m.account_id != old.account_id)
  AND EXISTS (SELECT 1 FROM channels c WHERE c.id = old.channel_id) BEGIN
    SELECT RAISE(ABORT, 'a channel keeps an owner');
END;
CREATE TRIGGER last_owner_stays_owner BEFORE UPDATE OF role ON members
WHEN old.role = 'owner' AND new.role != 'owner'
  AND NOT EXISTS (SELECT 1 FROM members m WHERE m.channel_id = old.channel_id
                  AND m.role = 'owner' AND m.account_id != old.account_id) BEGIN
    SELECT RAISE(ABORT, 'a channel keeps an owner');
END;

-- The triggers of before, on the columns of now.
CREATE TRIGGER rating_given AFTER INSERT ON likes BEGIN
    UPDATE videos SET like_count = like_count + (new.value = 1), dislike_count = dislike_count + (new.value = -1)
    WHERE id = new.video_id;
END;
CREATE TRIGGER rating_changed AFTER UPDATE OF value ON likes BEGIN
    UPDATE videos SET like_count = like_count - (old.value = 1) + (new.value = 1),
        dislike_count = dislike_count - (old.value = -1) + (new.value = -1)
    WHERE id = new.video_id;
END;
CREATE TRIGGER rating_taken_back AFTER DELETE ON likes BEGIN
    UPDATE videos SET like_count = like_count - (old.value = 1), dislike_count = dislike_count - (old.value = -1)
    WHERE id = old.video_id;
END;

CREATE TRIGGER subscribed AFTER INSERT ON subscriptions BEGIN
    UPDATE channels SET subscriber_count = subscriber_count + 1 WHERE id = new.channel_id;
END;
CREATE TRIGGER unsubscribed AFTER DELETE ON subscriptions BEGIN
    UPDATE channels SET subscriber_count = subscriber_count - 1 WHERE id = old.channel_id;
END;

-- A comment is hidden while the account that wrote it is blocked from the whole site, or its
-- channel is blocked by the channel of its video; see 0003 for the counts.
CREATE TRIGGER commented AFTER INSERT ON comments BEGIN
    UPDATE comments SET reply_count = reply_count + 1 WHERE id = new.parent_id AND new.hidden = 0;
    UPDATE videos SET comment_count = comment_count + 1 + new.reply_count
    WHERE id = new.video_id AND new.hidden = 0 AND new.parent_id IS NULL;
    UPDATE videos SET comment_count = comment_count + 1
    WHERE id = new.video_id AND new.hidden = 0 AND new.parent_id IS NOT NULL
      AND EXISTS (SELECT 1 FROM comments p WHERE p.id = new.parent_id AND p.hidden = 0);
    UPDATE comments SET hidden = 1 WHERE id = new.id AND hidden = 0 AND (
        EXISTS (SELECT 1 FROM accounts a WHERE a.id = new.account_id AND a.blocked = 1)
        OR EXISTS (SELECT 1 FROM blocks b JOIN videos v ON v.channel_id = b.channel_id
                   WHERE v.id = new.video_id AND b.blocked_id = new.channel_id));
END;
CREATE TRIGGER uncommented AFTER DELETE ON comments BEGIN
    UPDATE comments SET reply_count = reply_count - 1 WHERE id = old.parent_id AND old.hidden = 0;
    UPDATE videos SET comment_count = comment_count - 1 - old.reply_count
    WHERE id = old.video_id AND old.hidden = 0 AND old.parent_id IS NULL;
    UPDATE videos SET comment_count = comment_count - 1
    WHERE id = old.video_id AND old.hidden = 0 AND old.parent_id IS NOT NULL
      AND EXISTS (SELECT 1 FROM comments p WHERE p.id = old.parent_id AND p.hidden = 0);
END;
CREATE TRIGGER comment_hidden AFTER UPDATE OF hidden ON comments WHEN old.hidden != new.hidden BEGIN
    UPDATE comments SET reply_count = reply_count + old.hidden - new.hidden WHERE id = new.parent_id;
    UPDATE videos SET comment_count = comment_count + (old.hidden - new.hidden) * (1 + new.reply_count)
    WHERE id = new.video_id AND new.parent_id IS NULL;
    UPDATE videos SET comment_count = comment_count + old.hidden - new.hidden
    WHERE id = new.video_id AND new.parent_id IS NOT NULL
      AND EXISTS (SELECT 1 FROM comments p WHERE p.id = new.parent_id AND p.hidden = 0);
END;
CREATE TRIGGER channel_blocked AFTER INSERT ON blocks BEGIN
    UPDATE comments SET hidden = 1 WHERE channel_id = new.blocked_id AND hidden = 0
      AND video_id IN (SELECT id FROM videos WHERE channel_id = new.channel_id);
END;
CREATE TRIGGER channel_unblocked AFTER DELETE ON blocks BEGIN
    UPDATE comments SET hidden = 0 WHERE channel_id = old.blocked_id AND hidden = 1
      AND video_id IN (SELECT id FROM videos WHERE channel_id = old.channel_id)
      AND NOT EXISTS (SELECT 1 FROM accounts a WHERE a.id = comments.account_id AND a.blocked = 1);
END;
CREATE TRIGGER account_blocked AFTER UPDATE OF blocked ON accounts WHEN old.blocked != new.blocked BEGIN
    UPDATE comments SET hidden = 1 WHERE account_id = new.id AND hidden = 0 AND new.blocked = 1;
    UPDATE comments SET hidden = 0 WHERE account_id = new.id AND hidden = 1 AND new.blocked = 0
      AND NOT EXISTS (SELECT 1 FROM blocks b JOIN videos v ON v.channel_id = b.channel_id
                      WHERE v.id = comments.video_id AND b.blocked_id = comments.channel_id);
END;

CREATE TRIGGER comment_liked AFTER INSERT ON comment_likes BEGIN
    UPDATE comments SET like_count = like_count + 1 WHERE id = new.comment_id;
END;
CREATE TRIGGER comment_unliked AFTER DELETE ON comment_likes BEGIN
    UPDATE comments SET like_count = like_count - 1 WHERE id = old.comment_id;
END;

CREATE TRIGGER viewed AFTER INSERT ON views BEGIN
    UPDATE videos SET view_count = view_count + 1 WHERE id = new.video_id;
END;

CREATE TRIGGER video_indexed AFTER INSERT ON videos BEGIN
    INSERT INTO video_search (rowid, title, description, channel)
    SELECT new.id, new.title, new.description, c.name FROM channels c WHERE c.id = new.channel_id;
END;
CREATE TRIGGER video_reindexed AFTER UPDATE OF title, description ON videos BEGIN
    DELETE FROM video_search WHERE rowid = old.id;
    INSERT INTO video_search (rowid, title, description, channel)
    SELECT new.id, new.title, new.description, c.name FROM channels c WHERE c.id = new.channel_id;
END;
CREATE TRIGGER video_unindexed AFTER DELETE ON videos BEGIN
    DELETE FROM video_search WHERE rowid = old.id;
END;
CREATE TRIGGER channel_renamed AFTER UPDATE OF name ON channels BEGIN
    DELETE FROM video_search WHERE rowid IN (SELECT id FROM videos WHERE channel_id = new.id);
    INSERT INTO video_search (rowid, title, description, channel)
    SELECT v.id, v.title, v.description, new.name FROM videos v WHERE v.channel_id = new.id;
END;

CREATE TRIGGER stored_bytes_changed AFTER UPDATE OF state ON videos
WHEN new.state IS NOT old.state BEGIN
    UPDATE videos SET stored_bytes = NULL WHERE id = new.id;
END;
