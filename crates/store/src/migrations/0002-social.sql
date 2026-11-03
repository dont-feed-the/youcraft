-- What people do with videos: the indexes their pages read by, and the counts on videos,
-- channels and comments, kept by triggers so that no statement can forget one. Deleting a row
-- through ON DELETE CASCADE fires them too.

CREATE INDEX comments_liked ON comments (video_id, parent_id, like_count);
CREATE INDEX playlists_of_account ON playlists (account_id, updated_at DESC);
CREATE INDEX likes_recent ON likes (account_id, created_at DESC);
CREATE INDEX subscriptions_of_account ON subscriptions (account_id, created_at DESC);

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

-- A video counts its comments and their replies; a comment counts its replies.
CREATE TRIGGER commented AFTER INSERT ON comments BEGIN
    UPDATE videos SET comment_count = comment_count + 1 WHERE id = new.video_id;
    UPDATE comments SET reply_count = reply_count + 1 WHERE id = new.parent_id;
END;
CREATE TRIGGER uncommented AFTER DELETE ON comments BEGIN
    UPDATE videos SET comment_count = comment_count - 1 WHERE id = old.video_id;
    UPDATE comments SET reply_count = reply_count - 1 WHERE id = old.parent_id;
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

-- The search index follows the videos' titles and descriptions and their channels' names.
INSERT INTO video_search (rowid, title, description, channel)
SELECT v.id, v.title, v.description, c.name FROM videos v JOIN channels c ON c.id = v.channel_id;
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
