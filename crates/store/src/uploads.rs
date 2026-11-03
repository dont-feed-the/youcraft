//! Uploads in progress: the video they become, the bytes received, and the hand-over to a job.

use rusqlite::{OptionalExtension, params};
use youcraft_api::records::Visibility;
use youcraft_api::{AccountId, ChannelId, VideoId};

use crate::videos::count;
use crate::{Store, StoreError};

/// A video to upload, as `upload.create` makes it.
pub struct NewUpload<'a> {
    /// The video it becomes.
    pub video: VideoId,
    /// The channel it is uploaded to.
    pub channel: ChannelId,
    /// Its title.
    pub title: &'a str,
    /// Its description.
    pub description: &'a str,
    /// Who may see it once it plays.
    pub visibility: Visibility,
    /// The size of its file in bytes.
    pub size: u64,
    /// Now, in seconds since 1970.
    pub now: i64,
}

/// An upload in progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Receiving {
    /// The account whose channel it goes to.
    pub owner: AccountId,
    /// The size of its file in bytes.
    pub size: u64,
    /// Bytes received so far.
    pub received: u64,
}

fn signed(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

impl Store {
    /// The channel of `account`.
    pub fn channel_of(&self, account: AccountId) -> Result<Option<ChannelId>, StoreError> {
        let id: Option<i64> = self
            .connection()
            .query_row(
                "SELECT id FROM channels WHERE account_id = ?1",
                [account.to_i64()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id.map(ChannelId::from_i64))
    }

    /// The account whose channel published `video`.
    pub fn owner_of(&self, video: VideoId) -> Result<Option<AccountId>, StoreError> {
        let id: Option<i64> = self
            .connection()
            .query_row(
                "SELECT c.account_id FROM videos v JOIN channels c ON c.id = v.channel_id WHERE v.id = ?1",
                [video.to_i64()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id.map(AccountId::from_i64))
    }

    /// Create a video that is uploading, and its upload.
    pub fn create_upload(&self, upload: &NewUpload<'_>) -> Result<(), StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO videos (id, channel_id, title, description, visibility, state, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'uploading', ?6)",
            params![
                upload.video.to_i64(), upload.channel.to_i64(), upload.title, upload.description,
                upload.visibility.as_str(), upload.now
            ],
        )?;
        transaction.execute(
            "INSERT INTO uploads (id, video_id, size, created_at) VALUES (?1, ?1, ?2, ?3)",
            params![upload.video.to_i64(), signed(upload.size), upload.now],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// The upload of `video`, if it is still uploading.
    pub fn receiving(&self, video: VideoId) -> Result<Option<Receiving>, StoreError> {
        let row = self
            .connection()
            .query_row(
                "SELECT c.account_id, u.size, u.received FROM uploads u JOIN videos v ON v.id = u.video_id \
                 JOIN channels c ON c.id = v.channel_id WHERE u.video_id = ?1",
                [video.to_i64()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        Ok(row.map(|(owner, size, received)| Receiving {
            owner: AccountId::from_i64(owner),
            size: count(size),
            received: count(received),
        }))
    }

    /// The bytes that uploads started at or after `since` have yet to receive.
    pub fn promised(&self, since: i64) -> Result<u64, StoreError> {
        let owed: i64 = self.connection().query_row(
            "SELECT COALESCE(SUM(MAX(size - received, 0)), 0) FROM uploads WHERE created_at >= ?1",
            [since],
            |row| row.get(0),
        )?;
        Ok(count(owed))
    }

    /// Record that the upload of `video` has received `received` bytes.
    pub fn set_received(&self, video: VideoId, received: u64) -> Result<(), StoreError> {
        self.connection().execute(
            "UPDATE uploads SET received = ?2 WHERE video_id = ?1",
            params![video.to_i64(), signed(received)],
        )?;
        Ok(())
    }

    /// End the upload of `video`, whose bytes have all arrived, and queue the job that makes it
    /// playable; `false` when it is not uploading or bytes are missing.
    pub fn finish_upload(&self, video: VideoId, now: i64) -> Result<bool, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let removed = transaction.execute(
            "DELETE FROM uploads WHERE video_id = ?1 AND received = size",
            [video.to_i64()],
        )?;
        if removed == 0 {
            return Ok(false);
        }
        transaction.execute(
            "UPDATE videos SET state = 'processing' WHERE id = ?1",
            [video.to_i64()],
        )?;
        transaction.execute(
            "INSERT INTO jobs (video_id, created_at) VALUES (?1, ?2)",
            params![video.to_i64(), now],
        )?;
        transaction.commit()?;
        Ok(true)
    }
}
