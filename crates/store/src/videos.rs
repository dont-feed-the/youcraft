//! Videos: creating them, listing the watchable ones, and what the watch page shows.

use rusqlite::{OptionalExtension, Row, params};
use youcraft_api::records::{ChannelCard, Rating, VideoCard, VideoPage, Visibility};
use youcraft_api::time::MediaDuration;
use youcraft_api::{AccountId, ChannelId, VideoId};

use crate::{Store, StoreError};

/// A video already made into renditions, as a demo or a test inserts it.
pub struct NewVideo<'a> {
    /// Its identifier.
    pub id: VideoId,
    /// The channel that publishes it.
    pub channel: ChannelId,
    /// Its title.
    pub title: &'a str,
    /// Its description.
    pub description: &'a str,
    /// Who may see it.
    pub visibility: Visibility,
    /// How long it lasts.
    pub duration: MediaDuration,
    /// Its size in pixels.
    pub width: u32,
    /// Its size in pixels.
    pub height: u32,
    /// When it was published, in seconds since 1970.
    pub published_at: i64,
}

/// A watchable video with the public counts a recommendation algorithm may weigh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// The video as its card shows it.
    pub card: VideoCard,
    /// How many people liked it.
    pub likes: u64,
    /// How many people disliked it.
    pub dislikes: u64,
    /// How many comments it has.
    pub comments: u64,
}

/// The columns [`card`] reads, from `videos` as `v` joined with its `channels` as `c`.
pub(crate) const CARD_COLUMNS: &str = "v.id, v.title, v.duration_ticks, v.timescale, v.view_count, \
    COALESCE(v.published_at, v.created_at), c.id, c.handle, c.name";

/// The [`VideoCard`] of a row that starts with [`CARD_COLUMNS`].
pub(crate) fn card(row: &Row<'_>) -> rusqlite::Result<VideoCard> {
    Ok(VideoCard {
        id: VideoId::from_i64(row.get(0)?),
        title: row.get(1)?,
        duration: MediaDuration {
            ticks: count(row.get(2)?),
            timescale: u32::try_from(row.get::<_, i64>(3)?).unwrap_or(1000),
        },
        views: count(row.get(4)?),
        published_at: row.get(5)?,
        channel: ChannelCard {
            id: ChannelId::from_i64(row.get(6)?),
            handle: row.get(7)?,
            name: row.get(8)?,
        },
    })
}

/// A stored count, never negative.
pub(crate) fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

impl Store {
    /// Insert a video that is ready to play.
    pub fn insert_ready_video(&self, video: &NewVideo<'_>) -> Result<(), StoreError> {
        let connection = self.connection();
        let ticks = i64::try_from(video.duration.ticks).unwrap_or(i64::MAX);
        connection.execute(
            "INSERT INTO videos (id, channel_id, title, description, visibility, state, duration_ticks, timescale, \
             width, height, created_at, published_at) VALUES (?1, ?2, ?3, ?4, ?5, 'ready', ?6, ?7, ?8, ?9, ?10, ?10)",
            params![
                video.id.to_i64(), video.channel.to_i64(), video.title, video.description, video.visibility.as_str(),
                ticks, video.duration.timescale, video.width, video.height, video.published_at
            ],
        )?;
        Ok(())
    }

    /// The most recently published public videos that are ready, newest first, at most `limit`.
    pub fn recent_watchable(&self, limit: u32) -> Result<Vec<Listed>, StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {CARD_COLUMNS}, v.like_count, v.dislike_count, v.comment_count FROM videos v \
             JOIN channels c ON c.id = v.channel_id WHERE v.visibility = 'public' AND v.state = 'ready' \
             ORDER BY v.published_at DESC, v.id LIMIT ?1"
        ))?;
        let rows = statement.query_map([limit], |row| {
            Ok(Listed {
                card: card(row)?,
                likes: count(row.get(9)?),
                dislikes: count(row.get(10)?),
                comments: count(row.get(11)?),
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Who may watch `video`: its visibility and the account of its channel.
    pub fn audience(&self, video: VideoId) -> Result<Option<(Visibility, AccountId)>, StoreError> {
        let row: Option<(String, i64)> = self
            .connection()
            .query_row(
                "SELECT v.visibility, c.account_id FROM videos v JOIN channels c ON c.id = v.channel_id WHERE v.id = ?1",
                [video.to_i64()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((name, owner)) => {
                let visibility = Visibility::from_name(&name)
                    .ok_or_else(|| StoreError::Corrupt(format!("visibility {name}")))?;
                Ok(Some((visibility, AccountId::from_i64(owner))))
            }
        }
    }

    /// What the watch page shows about `video`, if `viewer` may see it: anyone a public or unlisted
    /// video, only its own channel a private one.
    pub fn video_page(
        &self,
        video: VideoId,
        viewer: Option<AccountId>,
    ) -> Result<Option<VideoPage>, StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {CARD_COLUMNS}, v.description, v.like_count, v.comment_count, v.comments_enabled, v.state, \
             v.visibility, c.account_id, c.subscriber_count, \
             (SELECT l.value FROM likes l WHERE l.video_id = v.id AND l.account_id = ?2), \
             EXISTS (SELECT 1 FROM subscriptions s WHERE s.channel_id = c.id AND s.account_id = ?2) \
             FROM videos v JOIN channels c ON c.id = v.channel_id WHERE v.id = ?1"
        ))?;
        let page = statement
            .query_row(
                params![video.to_i64(), viewer.map(AccountId::to_i64)],
                |row| {
                    let owner = AccountId::from_i64(row.get(15)?);
                    let owned = viewer == Some(owner);
                    let visible = row.get::<_, String>(14)? != "private" || owned;
                    Ok(visible.then_some(VideoPage {
                        card: card(row)?,
                        description: row.get(9)?,
                        likes: count(row.get(10)?),
                        comments: count(row.get(11)?),
                        comments_enabled: row.get::<_, i64>(12)? != 0,
                        playable: row.get::<_, String>(13)? == "ready",
                        subscribers: count(row.get(16)?),
                        rating: Rating::from_value(row.get(17)?),
                        subscribed: row.get(18)?,
                        owned,
                    }))
                },
            )
            .optional()?;
        Ok(page.flatten())
    }
}

#[cfg(test)]
mod tests {
    use youcraft_api::Role;

    use super::*;
    use crate::accounts::NewAccount;

    fn store_with_channel() -> (Store, AccountId, ChannelId) {
        let store = Store::in_memory().unwrap();
        let (account, channel) = (AccountId::from_bits(1), ChannelId::from_bits(2));
        store
            .create_account(&NewAccount {
                id: account,
                channel,
                google_subject: "g1",
                email: "a@example.com",
                name: "Ada",
                handle: "Ada",
                role: Role::Creator,
                now: 100,
            })
            .unwrap();
        (store, account, channel)
    }

    fn video(
        id: u64,
        channel: ChannelId,
        visibility: Visibility,
        published_at: i64,
    ) -> NewVideo<'static> {
        NewVideo {
            id: VideoId::from_bits(id),
            channel,
            title: "Waves",
            description: "About waves.",
            visibility,
            duration: MediaDuration::from_millis(61_000),
            width: 1920,
            height: 1080,
            published_at,
        }
    }

    #[test]
    fn only_public_ready_videos_are_listed_newest_first() {
        let (store, _, channel) = store_with_channel();
        store
            .insert_ready_video(&video(10, channel, Visibility::Public, 200))
            .unwrap();
        store
            .insert_ready_video(&video(11, channel, Visibility::Public, 300))
            .unwrap();
        store
            .insert_ready_video(&video(12, channel, Visibility::Unlisted, 400))
            .unwrap();
        store
            .insert_ready_video(&video(13, channel, Visibility::Private, 500))
            .unwrap();
        let listed: Vec<u64> = store
            .recent_watchable(10)
            .unwrap()
            .iter()
            .map(|l| l.card.id.bits())
            .collect();
        assert_eq!(listed, [11, 10]);
        let first = &store.recent_watchable(1).unwrap()[0].card;
        assert_eq!(first.duration.whole_seconds(), 61);
        assert_eq!(first.channel.handle, "Ada");
    }

    #[test]
    fn a_private_video_shows_to_its_channel_only() {
        let (store, owner, channel) = store_with_channel();
        store
            .insert_ready_video(&video(13, channel, Visibility::Private, 500))
            .unwrap();
        let id = VideoId::from_bits(13);
        assert!(store.video_page(id, None).unwrap().is_none());
        assert!(
            store
                .video_page(id, Some(AccountId::from_bits(99)))
                .unwrap()
                .is_none()
        );
        let page = store.video_page(id, Some(owner)).unwrap().unwrap();
        assert!(page.playable && page.comments_enabled);
        assert!(
            store
                .video_page(VideoId::from_bits(14), Some(owner))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn handles_are_unique_in_any_case() {
        let (store, _, channel) = store_with_channel();
        assert_eq!(store.channel_by_handle("ADA").unwrap(), Some(channel));
        let taken = store.create_account(&NewAccount {
            id: AccountId::from_bits(3),
            channel: ChannelId::from_bits(4),
            google_subject: "g2",
            email: "b@example.com",
            name: "Bob",
            handle: "aDa",
            role: Role::Viewer,
            now: 100,
        });
        assert!(taken.is_err());
    }

    #[test]
    fn search_indexes_every_script() {
        let store = Store::in_memory().unwrap();
        let connection = store.connection();
        connection
            .execute("INSERT INTO video_search (rowid, title, description, channel) VALUES (1, 'Ondes et lumière', '', 'Ada')", [])
            .unwrap();
        connection
            .execute("INSERT INTO video_search (rowid, title, description, channel) VALUES (2, '光の波について', '', 'Bob')", [])
            .unwrap();
        let find = |query: &str| -> Vec<i64> {
            let mut statement = connection
                .prepare("SELECT rowid FROM video_search WHERE video_search MATCH ?1")
                .unwrap();
            statement
                .query_map([query], |row| row.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect()
        };
        assert_eq!(find("lumiere"), [1], "diacritics fold");
        assert_eq!(find("の波に"), [2]);
    }
}
