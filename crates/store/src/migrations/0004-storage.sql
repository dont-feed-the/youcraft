-- What each video's files fill on disk, its original and its media, so that the room left in
-- the site's storage is a sum over rows rather than a walk over every file. NULL until the server
-- measures it; a video whose state changes has moved or made files, so it is measured again.
-- Bytes an upload has received are its row's `received`, outside this count until it finishes.

ALTER TABLE videos ADD COLUMN stored_bytes INTEGER CHECK (stored_bytes >= 0);

CREATE TRIGGER stored_bytes_changed AFTER UPDATE OF state ON videos
WHEN new.state IS NOT old.state BEGIN
    UPDATE videos SET stored_bytes = NULL WHERE id = new.id;
END;

CREATE INDEX videos_unmeasured ON videos (id) WHERE stored_bytes IS NULL;
CREATE INDEX uploads_by_age ON uploads (created_at);
