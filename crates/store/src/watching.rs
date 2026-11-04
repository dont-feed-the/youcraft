//! What a viewer does with a video, as the channel they act as: rating it, viewing it, and the
//! history of what it watched, from which the recommendation algorithm learns what it likes.

use rusqlite::{OptionalExtension, named_params, params};
use youcraft_api::commands::history::Watched;
use youcraft_api::records::Rating;
use youcraft_api::{ChannelId, VideoId};

use crate::videos::{CARD_COLUMNS, CARD_WIDTH, VISIBLE, card};
use crate::{Paged, Reader, Store, StoreError, Window, count};

/// What a channel has done that the recommendation algorithm may weigh, most recent first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Signals {
    /// The videos they watched, with their channel and when.
    pub watched: Vec<(VideoId, ChannelId, i64)>,
    /// The videos they liked.
    pub liked: Vec<VideoId>,
    /// The channels they subscribe to.
    pub subscribed: Vec<ChannelId>,
}

/// How many of each signal the algorithm receives.
const SIGNAL_LIMIT: i64 = 500;

impl Store {
    /// Give `video` the rating of the channel `reader` acts as, if they may see it; its likes then.
    pub fn rate(
        &self,
        reader: Reader,
        video: VideoId,
        rating: Rating,
        now: i64,
    ) -> Result<Option<u64>, StoreError> {
        let Some(channel) = reader.channel else {
            return Ok(None);
        };
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let seen: Option<i64> = transaction
            .query_row(
                &format!(
                    "SELECT v.id FROM videos v WHERE v.id = :video AND {VISIBLE}"
                ),
                named_params! {":video": video.to_i64(), ":reader": reader.id(), ":admin": reader.admin},
                |row| row.get(0),
            )
            .optional()?;
        if seen.is_none() {
            return Ok(None);
        }
        match rating.value() {
            Some(value) => transaction.execute(
                "INSERT INTO likes (channel_id, video_id, value, created_at) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT (channel_id, video_id) DO UPDATE SET value = excluded.value, \
                 created_at = CASE WHEN value = excluded.value THEN created_at ELSE excluded.created_at END",
                params![channel.to_i64(), video.to_i64(), value, now],
            )?,
            None => transaction.execute(
                "DELETE FROM likes WHERE channel_id = ?1 AND video_id = ?2",
                params![channel.to_i64(), video.to_i64()],
            )?,
        };
        let likes: i64 = transaction.query_row(
            "SELECT like_count FROM videos WHERE id = ?1",
            [video.to_i64()],
            |row| row.get(0),
        )?;
        transaction.commit()?;
        Ok(Some(count(likes)))
    }

    /// Count a view of `video` by the viewer known as `viewer` on `day`, once; whether it counted.
    /// The views of days before the previous one are forgotten: they can no longer repeat.
    pub fn count_view(&self, video: VideoId, viewer: &[u8], day: i64) -> Result<bool, StoreError> {
        let connection = self.connection();
        connection.execute(
            "DELETE FROM views WHERE video_id = ?1 AND day < ?2",
            params![video.to_i64(), day.saturating_sub(1)],
        )?;
        let counted = connection.execute(
            "INSERT OR IGNORE INTO views (video_id, viewer_hash, day) VALUES (?1, ?2, ?3)",
            params![video.to_i64(), viewer, day],
        )?;
        Ok(counted > 0)
    }

    /// Keep in the history of `channel` that it watched `video` now, unless it paused it.
    pub fn remember_watched(
        &self,
        channel: ChannelId,
        video: VideoId,
        now: i64,
    ) -> Result<(), StoreError> {
        self.connection().execute(
            "INSERT INTO history (channel_id, video_id, watched_at) \
             SELECT ?1, ?2, ?3 FROM channels WHERE id = ?1 AND history_paused = 0 \
             ON CONFLICT (channel_id, video_id) DO UPDATE SET watched_at = excluded.watched_at",
            params![channel.to_i64(), video.to_i64(), now],
        )?;
        Ok(())
    }

    /// The history of the channel `reader` acts as, the most recent first, with the videos they
    /// may still see; and whether it is paused. Someone who acts as no channel has none.
    pub fn history(
        &self,
        reader: Reader,
        window: Window,
    ) -> Result<(Paged<Watched>, bool), StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {CARD_COLUMNS}, h.watched_at FROM history h JOIN videos v ON v.id = h.video_id \
             JOIN channels c ON c.id = v.channel_id WHERE h.channel_id = :actor AND {VISIBLE} \
             ORDER BY h.watched_at DESC, v.id LIMIT :limit OFFSET :offset"
        ))?;
        let rows = statement.query_map(
            named_params! {
                ":reader": reader.id(), ":actor": reader.actor(), ":admin": reader.admin,
                ":limit": window.probe(), ":offset": window.skip()
            },
            |row| {
                Ok(Watched {
                    card: card(row)?,
                    watched_at: row.get(CARD_WIDTH)?,
                })
            },
        )?;
        let watched = window.page(rows.collect::<Result<_, _>>()?);
        let paused: Option<bool> = connection
            .query_row(
                "SELECT history_paused FROM channels WHERE id = ?1",
                [reader.actor()],
                |row| row.get(0),
            )
            .optional()?;
        Ok((watched, paused.unwrap_or(false)))
    }

    /// Take `video` out of the history of `channel`; whether it was there.
    pub fn forget_watched(&self, channel: ChannelId, video: VideoId) -> Result<bool, StoreError> {
        let removed = self.connection().execute(
            "DELETE FROM history WHERE channel_id = ?1 AND video_id = ?2",
            params![channel.to_i64(), video.to_i64()],
        )?;
        Ok(removed > 0)
    }

    /// Empty the history of `channel`.
    pub fn clear_history(&self, channel: ChannelId) -> Result<(), StoreError> {
        self.connection().execute(
            "DELETE FROM history WHERE channel_id = ?1",
            [channel.to_i64()],
        )?;
        Ok(())
    }

    /// Stop keeping the history of `channel`, or keep it again.
    pub fn pause_history(&self, channel: ChannelId, paused: bool) -> Result<(), StoreError> {
        self.connection().execute(
            "UPDATE channels SET history_paused = ?2 WHERE id = ?1",
            params![channel.to_i64(), i64::from(paused)],
        )?;
        Ok(())
    }

    /// What `channel` has done that the recommendation algorithm may weigh.
    pub fn signals(&self, channel: ChannelId) -> Result<Signals, StoreError> {
        let connection = self.connection();
        let id = channel.to_i64();
        let mut watched = connection.prepare_cached(
            "SELECT h.video_id, v.channel_id, h.watched_at FROM history h JOIN videos v ON v.id = h.video_id \
             WHERE h.channel_id = ?1 ORDER BY h.watched_at DESC LIMIT ?2",
        )?;
        let watched = watched
            .query_map(params![id, SIGNAL_LIMIT], |row| {
                Ok((
                    VideoId::from_i64(row.get(0)?),
                    ChannelId::from_i64(row.get(1)?),
                    row.get(2)?,
                ))
            })?
            .collect::<Result<_, _>>()?;
        let mut liked = connection.prepare_cached(
            "SELECT video_id FROM likes WHERE channel_id = ?1 AND value = 1 \
             ORDER BY created_at DESC LIMIT ?2",
        )?;
        let liked = liked
            .query_map(params![id, SIGNAL_LIMIT], |row| {
                Ok(VideoId::from_i64(row.get(0)?))
            })?
            .collect::<Result<_, _>>()?;
        let mut subscribed = connection.prepare_cached(
            "SELECT channel_id FROM subscriptions WHERE subscriber_id = ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let subscribed = subscribed
            .query_map(params![id, SIGNAL_LIMIT], |row| {
                Ok(ChannelId::from_i64(row.get(0)?))
            })?
            .collect::<Result<_, _>>()?;
        Ok(Signals {
            watched,
            liked,
            subscribed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{
        ADA_CHANNEL, BOB_CHANNEL, NEWER, PRIVATE, PUBLIC, UNLISTED, bob, with_videos,
    };

    #[test]
    fn a_rating_counts_once_and_changes_its_mind() {
        let store = with_videos();
        let rate = |rating| store.rate(bob(), PUBLIC, rating, 7).unwrap();
        assert_eq!(rate(Rating::Like), Some(1));
        assert_eq!(rate(Rating::Like), Some(1));
        assert_eq!(rate(Rating::Dislike), Some(0));
        let page = store.video_page(PUBLIC, bob()).unwrap().unwrap();
        assert_eq!(page.rating, Rating::Dislike);
        assert_eq!(rate(Rating::None), Some(0));
        assert_eq!(rate(Rating::Like), Some(1));
        let bob = bob();
        assert_eq!(store.rate(bob, UNLISTED, Rating::Like, 7).unwrap(), Some(1));
        assert_eq!(store.rate(bob, PRIVATE, Rating::Like, 7).unwrap(), None);
        assert_eq!(
            store.rate(Reader::ANYONE, PUBLIC, Rating::Like, 7).unwrap(),
            None,
            "only someone signed in rates"
        );
        assert_eq!(
            store.signals(BOB_CHANNEL).unwrap().liked,
            [PUBLIC, UNLISTED]
        );
    }

    #[test]
    fn a_viewer_counts_once_a_day() {
        let store = with_videos();
        assert!(store.count_view(PUBLIC, b"bob", 100).unwrap());
        assert!(!store.count_view(PUBLIC, b"bob", 100).unwrap());
        assert!(store.count_view(PUBLIC, b"eve", 100).unwrap());
        assert!(store.count_view(PUBLIC, b"bob", 101).unwrap());
        assert!(store.count_view(PUBLIC, b"bob", 103).unwrap());
        let views = store
            .video_page(PUBLIC, Reader::ANYONE)
            .unwrap()
            .unwrap()
            .card
            .views;
        assert_eq!(views, 4);
    }

    #[test]
    fn the_history_keeps_the_latest_watch_until_paused() {
        let store = with_videos();
        store.remember_watched(BOB_CHANNEL, PUBLIC, 10).unwrap();
        store.remember_watched(BOB_CHANNEL, NEWER, 11).unwrap();
        store.remember_watched(BOB_CHANNEL, PUBLIC, 12).unwrap();
        store.remember_watched(BOB_CHANNEL, PRIVATE, 13).unwrap();
        let all = Window {
            offset: 0,
            limit: 10,
        };
        let (history, paused) = store.history(bob(), all).unwrap();
        let ids: Vec<_> = history.items.iter().map(|w| w.card.id).collect();
        assert_eq!(ids, [PUBLIC, NEWER], "newest first, private videos hidden");
        assert!(!paused && !history.more);
        let signals = store.signals(BOB_CHANNEL).unwrap();
        assert_eq!(signals.watched.first(), Some(&(PRIVATE, ADA_CHANNEL, 13)));
        store.pause_history(BOB_CHANNEL, true).unwrap();
        store.remember_watched(BOB_CHANNEL, UNLISTED, 14).unwrap();
        let (history, paused) = store.history(bob(), all).unwrap();
        assert!(paused);
        assert_eq!(history.items.len(), 2, "nothing kept while paused");
        assert!(store.forget_watched(BOB_CHANNEL, NEWER).unwrap());
        assert!(!store.forget_watched(BOB_CHANNEL, NEWER).unwrap());
        store.clear_history(BOB_CHANNEL).unwrap();
        assert!(store.history(bob(), all).unwrap().0.items.is_empty());
    }
}
