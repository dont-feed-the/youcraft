-- The thumbnail a video's channel chose: the version of the files made from it, which its address
-- names, so that a new choice is a new address and caches keep each one for ever; NULL while the
-- video shows a frame of itself. Its files lie among the video's media, which `stored_bytes`
-- counts.
ALTER TABLE videos ADD COLUMN thumbnail TEXT;
