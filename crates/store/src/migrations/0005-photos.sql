-- A channel's photo: the version of the files made from it, which its address names, so that a
-- new photo is a new address and caches keep each one for ever; NULL while it has none.
ALTER TABLE channels ADD COLUMN photo TEXT;

-- What the files of the channel's photo fill on disk, which the site's storage counts.
ALTER TABLE channels ADD COLUMN photo_bytes INTEGER NOT NULL DEFAULT 0 CHECK (photo_bytes >= 0);
