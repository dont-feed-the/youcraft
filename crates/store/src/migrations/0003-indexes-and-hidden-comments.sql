-- The indexes that the rows referring to others need, and comments that hide.

-- Every column that refers to another table's row, for the deletes that follow references (with
-- ON DELETE CASCADE) and for the queries that read by them; the others already lead an index.
CREATE INDEX comments_replies ON comments (parent_id, created_at);
CREATE INDEX comments_of_account ON comments (account_id);
CREATE INDEX comment_likes_of_comment ON comment_likes (comment_id);
CREATE INDEX blocks_of_account ON blocks (account_id);
CREATE INDEX playlist_items_of_video ON playlist_items (video_id);
CREATE INDEX history_of_video ON history (video_id);
CREATE INDEX jobs_of_video ON jobs (video_id, id);

-- A comment is hidden while its author is blocked from the whole site or by the channel of its
-- video. A hidden comment, and every reply to a hidden comment, shows to nobody and counts
-- nowhere. `hidden` keeps that fact on the row, and the triggers below keep it true.
ALTER TABLE comments ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1));
UPDATE comments SET hidden = 1
WHERE EXISTS (SELECT 1 FROM accounts a WHERE a.id = comments.account_id AND a.blocked = 1)
   OR EXISTS (SELECT 1 FROM blocks b JOIN videos v ON v.channel_id = b.channel_id
              WHERE v.id = comments.video_id AND b.account_id = comments.account_id);

-- The counts, as the comments now count: a comment, the replies to it that are not hidden; a
-- video, its comments that are not hidden, each with its count of replies. With h the hidden flag
-- and r the count of replies, a video counts the sum of 1 + r(m) over its top-level comments m
-- with h(m) = 0; each trigger below adds what one change of a row changes in that sum.
UPDATE comments SET reply_count =
    (SELECT count(*) FROM comments r WHERE r.parent_id = comments.id AND r.hidden = 0);
UPDATE videos SET comment_count = (
    SELECT count(*) FROM comments m WHERE m.video_id = videos.id AND m.hidden = 0
    AND (m.parent_id IS NULL OR EXISTS (SELECT 1 FROM comments p WHERE p.id = m.parent_id AND p.hidden = 0)));

DROP TRIGGER commented;
DROP TRIGGER uncommented;

-- A new comment counts as shown, then hides at once if its author is blocked. A comment deleted
-- with its top-level comment (ON DELETE CASCADE) finds no parent and changes nothing: the
-- top-level comment took its replies off the video's count as it went.
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
                   WHERE v.id = new.video_id AND b.account_id = new.account_id));
END;
CREATE TRIGGER uncommented AFTER DELETE ON comments BEGIN
    UPDATE comments SET reply_count = reply_count - 1 WHERE id = old.parent_id AND old.hidden = 0;
    UPDATE videos SET comment_count = comment_count - 1 - old.reply_count
    WHERE id = old.video_id AND old.hidden = 0 AND old.parent_id IS NULL;
    UPDATE videos SET comment_count = comment_count - 1
    WHERE id = old.video_id AND old.hidden = 0 AND old.parent_id IS NOT NULL
      AND EXISTS (SELECT 1 FROM comments p WHERE p.id = old.parent_id AND p.hidden = 0);
END;
-- A comment that hides takes itself off the counts, and a top-level one its replies too; one that
-- shows again puts them back. Its replies keep their own flags.
CREATE TRIGGER comment_hidden AFTER UPDATE OF hidden ON comments WHEN old.hidden != new.hidden BEGIN
    UPDATE comments SET reply_count = reply_count + old.hidden - new.hidden WHERE id = new.parent_id;
    UPDATE videos SET comment_count = comment_count + (old.hidden - new.hidden) * (1 + new.reply_count)
    WHERE id = new.video_id AND new.parent_id IS NULL;
    UPDATE videos SET comment_count = comment_count + old.hidden - new.hidden
    WHERE id = new.video_id AND new.parent_id IS NOT NULL
      AND EXISTS (SELECT 1 FROM comments p WHERE p.id = new.parent_id AND p.hidden = 0);
END;

-- Blocks hide and show the comments of the account blocked.
CREATE TRIGGER channel_blocked AFTER INSERT ON blocks BEGIN
    UPDATE comments SET hidden = 1 WHERE account_id = new.account_id AND hidden = 0
      AND video_id IN (SELECT id FROM videos WHERE channel_id = new.channel_id);
END;
CREATE TRIGGER channel_unblocked AFTER DELETE ON blocks BEGIN
    UPDATE comments SET hidden = 0 WHERE account_id = old.account_id AND hidden = 1
      AND video_id IN (SELECT id FROM videos WHERE channel_id = old.channel_id)
      AND NOT EXISTS (SELECT 1 FROM accounts a WHERE a.id = old.account_id AND a.blocked = 1);
END;
CREATE TRIGGER account_blocked AFTER UPDATE OF blocked ON accounts WHEN old.blocked != new.blocked BEGIN
    UPDATE comments SET hidden = 1 WHERE account_id = new.id AND hidden = 0 AND new.blocked = 1;
    UPDATE comments SET hidden = 0 WHERE account_id = new.id AND hidden = 1 AND new.blocked = 0
      AND NOT EXISTS (SELECT 1 FROM blocks b JOIN videos v ON v.channel_id = b.channel_id
                      WHERE v.id = comments.video_id AND b.account_id = new.id);
END;
