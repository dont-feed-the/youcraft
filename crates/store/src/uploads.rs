//! Uploads in progress: the room they claim, the video they become, the bytes received, the
//! hand-over to a job, and when they lapse.
//!
//! The room left in the site's storage is counted from rows, never from a walk over the disk:
//! what each video's files fill (`stored_bytes`, which the server measures once after each
//! change), what uploads have received, and what open ones have yet to receive.

use rusqlite::{OptionalExtension, params};
use youcraft_api::records::Visibility;
use youcraft_api::{AccountId, ChannelId, VideoId};

use crate::{Store, StoreError, count, signed};

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
    /// When it started, in seconds since 1970.
    pub created_at: i64,
}

/// What a new upload may claim of the site's storage and of its channel's share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Allowance {
    /// The most bytes the site's videos may fill, uploads in progress included; `u64::MAX` for
    /// no limit.
    pub storage: u64,
    /// Bytes on disk that no row counts: transcodes in progress.
    pub unrecorded: u64,
    /// The largest file one may upload.
    pub file: u64,
    /// The most uploads one channel may have open at once.
    pub open: u64,
    /// The most bytes one channel's open uploads may claim together.
    pub open_bytes: u64,
    /// When the oldest upload still open started; an older one has lapsed and claims nothing.
    pub since: i64,
}

impl Allowance {
    /// No limit at all, for the demo and tests.
    pub const UNBOUNDED: Allowance = Allowance {
        storage: u64::MAX,
        unrecorded: 0,
        file: u64::MAX,
        open: u64::MAX,
        open_bytes: u64::MAX,
        since: i64::MIN,
    };
}

/// Whether an upload could start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    /// It started, and claims its size.
    Started,
    /// Its channel has as many uploads open as it may.
    Crowded,
    /// It is larger than `limit`, the most it may be now.
    TooLarge {
        /// The largest size that would fit.
        limit: u64,
    },
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

    /// Create a video that is uploading, and its upload, if `allowance` leaves room for it.
    ///
    /// The room is the storage limit less what videos fill (their measured `stored_bytes`), the
    /// bytes uploads have received, what open uploads have yet to receive and what no row counts;
    /// measuring it and claiming from it happen in one transaction, so that two uploads, from
    /// this process or another, cannot both take the same room.
    pub fn create_upload(
        &self,
        upload: &NewUpload<'_>,
        allowance: &Allowance,
    ) -> Result<Claim, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let (open, claimed): (i64, i64) = transaction.query_row(
            "SELECT COUNT(*), COALESCE(SUM(u.size), 0) FROM uploads u JOIN videos v ON v.id = u.video_id \
             WHERE v.channel_id = ?1 AND u.created_at >= ?2",
            params![upload.channel.to_i64(), allowance.since],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if count(open) >= allowance.open {
            return Ok(Claim::Crowded);
        }
        let room = if allowance.storage == u64::MAX {
            u64::MAX
        } else {
            let (stored, received, owed): (i64, i64, i64) = transaction.query_row(
                "SELECT (SELECT COALESCE(SUM(stored_bytes), 0) FROM videos), \
                 (SELECT COALESCE(SUM(received), 0) FROM uploads), \
                 (SELECT COALESCE(SUM(MAX(size - received, 0)), 0) FROM uploads WHERE created_at >= ?1)",
                [allowance.since],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            let taken = [
                count(stored),
                count(received),
                count(owed),
                allowance.unrecorded,
            ]
            .into_iter()
            .fold(0, u64::saturating_add);
            allowance.storage.saturating_sub(taken)
        };
        let limit = allowance
            .file
            .min(room)
            .min(allowance.open_bytes.saturating_sub(count(claimed)));
        if upload.size > limit {
            return Ok(Claim::TooLarge { limit });
        }
        transaction.execute(
            "INSERT INTO videos (id, channel_id, title, description, visibility, state, created_at, stored_bytes) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'uploading', ?6, 0)",
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
        Ok(Claim::Started)
    }

    /// The upload of `video`, if it is still uploading.
    pub fn receiving(&self, video: VideoId) -> Result<Option<Receiving>, StoreError> {
        let row = self
            .connection()
            .query_row(
                "SELECT c.account_id, u.size, u.received, u.created_at FROM uploads u \
                 JOIN videos v ON v.id = u.video_id JOIN channels c ON c.id = v.channel_id \
                 WHERE u.video_id = ?1",
                [video.to_i64()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        Ok(row.map(|(owner, size, received, created_at)| Receiving {
            owner: AccountId::from_i64(owner),
            size: count(size),
            received: count(received),
            created_at,
        }))
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

    /// Up to `limit` videos whose files have not been measured since they last changed.
    pub fn unmeasured(&self, limit: u32) -> Result<Vec<VideoId>, StoreError> {
        let connection = self.connection();
        let mut statement = connection
            .prepare_cached("SELECT id FROM videos WHERE stored_bytes IS NULL LIMIT ?1")?;
        let ids = statement
            .query_map([limit], |row| row.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        Ok(ids.into_iter().map(VideoId::from_i64).collect())
    }

    /// Record that the files of `video` fill `bytes`, unless they were measured meanwhile.
    pub fn set_stored_bytes(&self, video: VideoId, bytes: u64) -> Result<(), StoreError> {
        self.connection().execute(
            "UPDATE videos SET stored_bytes = ?2 WHERE id = ?1 AND stored_bytes IS NULL",
            params![video.to_i64(), signed(bytes)],
        )?;
        Ok(())
    }

    /// Up to `limit` videos whose uploads started before `since` and lapsed unfinished.
    pub fn lapsed_uploads(&self, since: i64, limit: u32) -> Result<Vec<VideoId>, StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(
            "SELECT video_id FROM uploads WHERE created_at < ?1 ORDER BY created_at LIMIT ?2",
        )?;
        let ids = statement
            .query_map(params![since, limit], |row| row.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        Ok(ids.into_iter().map(VideoId::from_i64).collect())
    }

    /// Delete `video`, if it is still uploading and its upload started before `since`;
    /// `false` when it was not.
    pub fn drop_lapsed_upload(&self, video: VideoId, since: i64) -> Result<bool, StoreError> {
        let deleted = self.connection().execute(
            "DELETE FROM videos WHERE id = ?1 AND state = 'uploading' \
             AND EXISTS (SELECT 1 FROM uploads WHERE video_id = ?1 AND created_at < ?2)",
            params![video.to_i64(), since],
        )?;
        Ok(deleted > 0)
    }
}

#[cfg(test)]
mod tests {
    use youcraft_api::Grant;
    use youcraft_api::records::Failure;

    use super::*;
    use crate::accounts::NewAccount;

    fn store_with_channel(id: u64) -> (Store, ChannelId) {
        let store = Store::in_memory().unwrap();
        let channel = ChannelId::from_bits(id + 1);
        store
            .create_account(&NewAccount {
                id: AccountId::from_bits(id),
                channel,
                google_subject: "g",
                email: "ada@example.com",
                name: "Ada",
                handle: "ada",
                role: Grant::Creator,
                now: 0,
            })
            .unwrap();
        (store, channel)
    }

    fn upload(channel: ChannelId, id: u64, size: u64, now: i64) -> NewUpload<'static> {
        NewUpload {
            video: VideoId::from_bits(id),
            channel,
            title: "Waves",
            description: "",
            visibility: Visibility::Private,
            size,
            now,
        }
    }

    const ROOM: Allowance = Allowance {
        storage: 1000,
        unrecorded: 0,
        file: 600,
        open: 3,
        open_bytes: 900,
        since: 100,
    };

    #[test]
    fn an_upload_claims_its_size_from_what_videos_and_uploads_leave() {
        let (store, channel) = store_with_channel(1);
        let claim = |id, size| store.create_upload(&upload(channel, id, size, 100), &ROOM);
        assert_eq!(claim(10, 601).unwrap(), Claim::TooLarge { limit: 600 });
        assert_eq!(claim(10, 500).unwrap(), Claim::Started);
        store.set_received(VideoId::from_bits(10), 200).unwrap();
        assert_eq!(
            claim(11, 450).unwrap(),
            Claim::TooLarge { limit: 400 },
            "the channel's share: 900 less the 500 claimed"
        );
        assert_eq!(claim(11, 400).unwrap(), Claim::Started);
        assert_eq!(claim(12, 1).unwrap(), Claim::TooLarge { limit: 0 });
        let wide = Allowance {
            open_bytes: u64::MAX,
            ..ROOM
        };
        assert_eq!(
            store
                .create_upload(&upload(channel, 12, 101, 100), &wide)
                .unwrap(),
            Claim::TooLarge { limit: 100 },
            "the site's room: received and owed bytes both count"
        );
        let unrecorded = Allowance {
            unrecorded: 50,
            open_bytes: u64::MAX,
            ..ROOM
        };
        let (other, theirs) = store_with_channel(5);
        other
            .create_upload(&upload(theirs, 20, 900, 100), &Allowance::UNBOUNDED)
            .unwrap();
        assert_eq!(
            other
                .create_upload(&upload(theirs, 21, 60, 100), &unrecorded)
                .unwrap(),
            Claim::TooLarge { limit: 50 }
        );
    }

    #[test]
    fn a_channel_keeps_only_so_many_uploads_open() {
        let (store, channel) = store_with_channel(1);
        for id in 10..13 {
            let started = store.create_upload(&upload(channel, id, 1, 100), &ROOM);
            assert_eq!(started.unwrap(), Claim::Started);
        }
        let fourth = store.create_upload(&upload(channel, 13, 1, 100), &ROOM);
        assert_eq!(fourth.unwrap(), Claim::Crowded);
        assert!(
            !store.finish_upload(VideoId::from_bits(11), 101).unwrap(),
            "bytes are missing"
        );
        store.set_received(VideoId::from_bits(11), 1).unwrap();
        assert!(store.finish_upload(VideoId::from_bits(11), 101).unwrap());
        let again = store.create_upload(&upload(channel, 13, 1, 100), &ROOM);
        assert_eq!(
            again.unwrap(),
            Claim::Started,
            "a finished upload is closed"
        );
    }

    #[test]
    fn a_lapsed_upload_claims_nothing_and_is_dropped() {
        let (store, channel) = store_with_channel(1);
        let old = VideoId::from_bits(10);
        store
            .create_upload(&upload(channel, 10, 600, 50), &Allowance::UNBOUNDED)
            .unwrap();
        store.set_received(old, 100).unwrap();
        let fits = store.create_upload(&upload(channel, 11, 600, 100), &ROOM);
        assert_eq!(
            fits.unwrap(),
            Claim::Started,
            "its 500 owed bytes lapsed, its 100 received ones stay"
        );
        let full = store.create_upload(&upload(channel, 12, 301, 100), &ROOM);
        assert_eq!(full.unwrap(), Claim::TooLarge { limit: 300 });
        assert_eq!(store.lapsed_uploads(100, 10).unwrap(), vec![old]);
        assert!(
            !store
                .drop_lapsed_upload(VideoId::from_bits(11), 100)
                .unwrap()
        );
        assert!(store.drop_lapsed_upload(old, 100).unwrap());
        assert!(store.receiving(old).unwrap().is_none());
        assert!(store.lapsed_uploads(100, 10).unwrap().is_empty());
    }

    #[test]
    fn videos_are_measured_again_after_each_change() {
        let (store, channel) = store_with_channel(1);
        let video = VideoId::from_bits(10);
        store
            .create_upload(&upload(channel, 10, 5, 100), &Allowance::UNBOUNDED)
            .unwrap();
        assert!(
            store.unmeasured(10).unwrap().is_empty(),
            "an upload's bytes are its row's"
        );
        store.set_received(video, 5).unwrap();
        assert!(store.finish_upload(video, 101).unwrap());
        assert_eq!(store.unmeasured(10).unwrap(), vec![video]);
        store.set_stored_bytes(video, 5).unwrap();
        assert!(store.unmeasured(10).unwrap().is_empty());
        let room = Allowance {
            open_bytes: u64::MAX,
            file: u64::MAX,
            ..ROOM
        };
        let claim = store.create_upload(&upload(channel, 11, 996, 100), &room);
        assert_eq!(claim.unwrap(), Claim::TooLarge { limit: 995 });
        let job = store.claim_job(102).unwrap().unwrap();
        assert_eq!(
            store.unmeasured(10).unwrap(),
            Vec::<VideoId>::new(),
            "processing is no change of files"
        );
        store.job_failed(job, Failure::Decode, 103).unwrap();
        assert_eq!(store.unmeasured(10).unwrap(), vec![video]);
    }
}
