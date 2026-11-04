//! Videos: creating them, listing the watchable ones, and what the watch page shows; and the one
//! rule of who may see a video, which every read of the store applies.

use rusqlite::{OptionalExtension, Row, named_params, params};
use youcraft_api::records::{Rating, VideoCard, VideoPage, Visibility};
use youcraft_api::time::MediaDuration;
use youcraft_api::{ChannelId, VideoId};

use crate::channels::channel_card;
use crate::{Name, Named, Reader, Store, StoreError, count, signed};

/// Whether everyone may see the video `v`: it is ready, and public or unlisted.
macro_rules! seen_by_everyone {
    () => {
        "v.state = 'ready' AND v.visibility != 'private'"
    };
}

/// Whether the reader `:reader`, an administrator when `:admin`, may see the video `v`: everyone
/// may, or they manage it (they run the site, or it is their channel's). The channel is looked
/// up once per statement, not once per video.
pub(crate) const VISIBLE: &str = concat!(
    "((",
    seen_by_everyone!(),
    ") OR :admin OR v.channel_id IN (SELECT id FROM channels WHERE account_id = :reader))"
);

/// Whether feeds, search and its channel's page list the video `v`: it is ready and public.
pub(crate) const LISTED: &str = "v.visibility = 'public' AND v.state = 'ready'";

/// Where a video is on its way to playing, as `videos.state` keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    /// Its file is arriving.
    Uploading,
    /// A job is making its renditions.
    Processing,
    /// It plays.
    Ready,
    /// It could not be made to play.
    Failed,
}

impl Name for State {
    const WHAT: &'static str = "video state";
    fn from_name(name: &str) -> Option<State> {
        Some(match name {
            "uploading" => State::Uploading,
            "processing" => State::Processing,
            "ready" => State::Ready,
            "failed" => State::Failed,
            _ => return None,
        })
    }
}

/// Who may see a video, as its reader may learn it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Audience {
    /// Everyone: it is ready, and public or unlisted.
    Everyone,
    /// Only those who manage it, its channel and the site's administrators, of whom the reader is
    /// one.
    Managers,
}

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
        channel: channel_card(row, 6)?,
    })
}

impl Store {
    /// Insert a video that is ready to play.
    pub fn insert_ready_video(&self, video: &NewVideo<'_>) -> Result<(), StoreError> {
        let connection = self.connection();
        let ticks = signed(video.duration.ticks);
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
             JOIN channels c ON c.id = v.channel_id WHERE {LISTED} \
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

    /// Who may see `video`, if `reader` may.
    pub fn audience(&self, video: VideoId, reader: Reader) -> Result<Option<Audience>, StoreError> {
        let everyone: Option<bool> = self
            .connection()
            .query_row(
                &format!(
                    "SELECT {} FROM videos v WHERE v.id = :video AND {VISIBLE}",
                    seen_by_everyone!()
                ),
                named_params! {":video": video.to_i64(), ":reader": reader.id(), ":admin": reader.admin},
                |row| row.get(0),
            )
            .optional()?;
        Ok(everyone.map(|everyone| {
            if everyone {
                Audience::Everyone
            } else {
                Audience::Managers
            }
        }))
    }

    /// What the watch page shows about `video`, if `reader` may see it.
    pub fn video_page(
        &self,
        video: VideoId,
        reader: Reader,
    ) -> Result<Option<VideoPage>, StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {CARD_COLUMNS}, v.description, v.like_count, v.comment_count, v.comments_enabled, v.state, \
             c.account_id IS :reader, c.subscriber_count, \
             (SELECT l.value FROM likes l WHERE l.video_id = v.id AND l.account_id = :reader), \
             EXISTS (SELECT 1 FROM subscriptions s WHERE s.channel_id = c.id AND s.account_id = :reader) \
             FROM videos v JOIN channels c ON c.id = v.channel_id WHERE v.id = :video AND {VISIBLE}"
        ))?;
        let page = statement
            .query_row(
                named_params! {":video": video.to_i64(), ":reader": reader.id(), ":admin": reader.admin},
                |row| {
                    Ok(VideoPage {
                        card: card(row)?,
                        description: row.get(9)?,
                        likes: count(row.get(10)?),
                        comments: count(row.get(11)?),
                        comments_enabled: row.get::<_, i64>(12)? != 0,
                        playable: row.get::<_, Named<State>>(13)?.0 == State::Ready,
                        owned: row.get(14)?,
                        subscribers: count(row.get(15)?),
                        rating: Rating::from_value(row.get(16)?),
                        subscribed: row.get(17)?,
                    })
                },
            )
            .optional()?;
        Ok(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{ADA, ADA_CHANNEL, BOB, NEWER, PRIVATE, PUBLIC, UNLISTED, with_videos};
    use crate::uploads::NewUpload;

    const ADMIN: Reader = Reader {
        account: None,
        admin: true,
    };

    #[test]
    fn only_public_ready_videos_are_listed_newest_first() {
        let store = with_videos();
        let listed: Vec<VideoId> = store
            .recent_watchable(10)
            .unwrap()
            .iter()
            .map(|l| l.card.id)
            .collect();
        assert_eq!(listed, [NEWER, PUBLIC]);
        let first = &store.recent_watchable(1).unwrap()[0].card;
        assert_eq!(first.duration.whole_seconds(), 61);
        assert_eq!(first.channel.handle, "Ada");
    }

    #[test]
    fn a_video_shows_to_whoever_manages_it_and_to_everyone_once_it_plays_unless_private() {
        let store = with_videos();
        let seen = |video, reader| store.video_page(video, reader).unwrap().is_some();
        for video in [PUBLIC, UNLISTED] {
            assert!(seen(video, Reader::ANYONE) && seen(video, Reader::of(BOB)));
            assert_eq!(
                store.audience(video, Reader::ANYONE).unwrap(),
                Some(Audience::Everyone)
            );
        }
        assert!(!seen(PRIVATE, Reader::ANYONE) && !seen(PRIVATE, Reader::of(BOB)));
        assert!(seen(PRIVATE, Reader::of(ADA)) && seen(PRIVATE, ADMIN));
        assert_eq!(store.audience(PRIVATE, Reader::of(BOB)).unwrap(), None);
        assert_eq!(
            store.audience(PRIVATE, ADMIN).unwrap(),
            Some(Audience::Managers)
        );
        let page = store.video_page(PRIVATE, Reader::of(ADA)).unwrap().unwrap();
        assert!(page.owned && page.playable && page.comments_enabled);
        assert!(!store.video_page(PRIVATE, ADMIN).unwrap().unwrap().owned);
        assert!(!seen(VideoId::from_bits(14), Reader::of(ADA)));

        let uploading = VideoId::from_bits(20);
        store
            .create_upload(&NewUpload {
                video: uploading,
                channel: ADA_CHANNEL,
                title: "Soon",
                description: "",
                visibility: Visibility::Public,
                size: 10,
                now: 600,
            })
            .unwrap();
        assert!(!seen(uploading, Reader::ANYONE), "public, but not ready");
        let page = store
            .video_page(uploading, Reader::of(ADA))
            .unwrap()
            .unwrap();
        assert!(page.owned && !page.playable);
        assert_eq!(
            store.audience(uploading, Reader::of(ADA)).unwrap(),
            Some(Audience::Managers)
        );
    }
}
