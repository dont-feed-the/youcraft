//! What the studio shows and changes: a channel's videos in every state, edits, deletion.

use rusqlite::{OptionalExtension, Row, params};
use youcraft_api::records::{Processing, StudioVideo, Visibility};
use youcraft_api::{ChannelId, VideoId};

use crate::videos::{CARD_COLUMNS, State, card};
use crate::{Named, Store, StoreError, count};

/// What to change about a video; what is `None` stays.
#[derive(Clone, Copy, Debug, Default)]
pub struct VideoEdit<'a> {
    /// Its new title.
    pub title: Option<&'a str>,
    /// Its new description.
    pub description: Option<&'a str>,
    /// Who may now see it.
    pub visibility: Option<Visibility>,
    /// Whether people may now comment on it.
    pub comments_enabled: Option<bool>,
}

const STUDIO_COLUMNS: &str = "v.description, v.visibility, v.state, v.failure, v.comments_enabled, v.like_count, \
    v.comment_count, u.received, u.size, (SELECT j.progress_permille FROM jobs j WHERE j.video_id = v.id \
    ORDER BY j.id DESC LIMIT 1)";
const STUDIO_FROM: &str =
    "FROM videos v JOIN channels c ON c.id = v.channel_id LEFT JOIN uploads u ON u.video_id = v.id";

fn studio_video(row: &Row<'_>) -> rusqlite::Result<StudioVideo> {
    let processing = match row.get::<_, Named<State>>(11)?.0 {
        State::Uploading => Processing::Uploading {
            received: count(row.get::<_, Option<i64>>(16)?.unwrap_or(0)),
            size: count(row.get::<_, Option<i64>>(17)?.unwrap_or(0)),
        },
        State::Processing => Processing::Processing {
            permille: u32::try_from(row.get::<_, Option<i64>>(18)?.unwrap_or(0)).unwrap_or(0),
        },
        State::Ready => Processing::Ready,
        State::Failed => Processing::Failed {
            reason: row
                .get::<_, Option<String>>(12)?
                .unwrap_or_else(|| "internal".to_owned()),
        },
    };
    Ok(StudioVideo {
        card: card(row)?,
        description: row.get(9)?,
        visibility: row.get::<_, Named<Visibility>>(10)?.0,
        processing,
        comments_enabled: row.get::<_, i64>(13)? != 0,
        likes: count(row.get(14)?),
        comments: count(row.get(15)?),
    })
}

impl Store {
    /// The videos of `channel` in every state, newest first, from the one after `after`, at
    /// most `limit`.
    pub fn channel_videos(
        &self,
        channel: ChannelId,
        after: Option<VideoId>,
        limit: u32,
    ) -> Result<Vec<StudioVideo>, StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {CARD_COLUMNS}, {STUDIO_COLUMNS} {STUDIO_FROM} WHERE v.channel_id = ?1 AND (?2 IS NULL OR \
             (v.created_at, v.id) < (SELECT created_at, id FROM videos WHERE id = ?2)) \
             ORDER BY v.created_at DESC, v.id DESC LIMIT ?3"
        ))?;
        let after = after.map(VideoId::to_i64);
        let rows = statement.query_map(params![channel.to_i64(), after, limit], studio_video)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// `video` as the studio shows it.
    pub fn studio_video(&self, video: VideoId) -> Result<Option<StudioVideo>, StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {CARD_COLUMNS}, {STUDIO_COLUMNS} {STUDIO_FROM} WHERE v.id = ?1"
        ))?;
        Ok(statement
            .query_row([video.to_i64()], studio_video)
            .optional()?)
    }

    /// Change `video` as `edit` says; a ready video made public for the first time is published
    /// now.
    pub fn edit_video(
        &self,
        video: VideoId,
        edit: &VideoEdit<'_>,
        now: i64,
    ) -> Result<(), StoreError> {
        self.connection().execute(
            "UPDATE videos SET title = COALESCE(?2, title), description = COALESCE(?3, description), \
             visibility = COALESCE(?4, visibility), comments_enabled = COALESCE(?5, comments_enabled), \
             published_at = CASE WHEN COALESCE(?4, visibility) = 'public' AND state = 'ready' \
             THEN COALESCE(published_at, ?6) ELSE published_at END WHERE id = ?1",
            params![
                video.to_i64(), edit.title, edit.description, edit.visibility.map(Visibility::as_str),
                edit.comments_enabled, now
            ],
        )?;
        Ok(())
    }

    /// Date the publication of `video`, as a demo or an import from elsewhere does.
    pub fn set_published_at(&self, video: VideoId, at: i64) -> Result<(), StoreError> {
        self.connection().execute(
            "UPDATE videos SET published_at = ?2 WHERE id = ?1",
            params![video.to_i64(), at],
        )?;
        Ok(())
    }

    /// Delete `video` and everything that refers to it.
    pub fn delete_video(&self, video: VideoId) -> Result<(), StoreError> {
        self.connection()
            .execute("DELETE FROM videos WHERE id = ?1", [video.to_i64()])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use youcraft_api::time::MediaDuration;
    use youcraft_api::{AccountId, Grant};

    use super::*;
    use crate::accounts::NewAccount;
    use crate::jobs::{Made, MadeRendition};
    use crate::uploads::NewUpload;

    fn store() -> (Store, AccountId, ChannelId) {
        let store = Store::in_memory().unwrap();
        let (account, channel) = (AccountId::from_bits(1), ChannelId::from_bits(2));
        store
            .create_account(&NewAccount {
                id: account,
                channel,
                google_subject: "g1",
                email: "a@example.com",
                name: "Ada",
                handle: "ada",
                role: Grant::Creator,
                now: 100,
            })
            .unwrap();
        (store, account, channel)
    }

    fn upload(store: &Store, channel: ChannelId, id: u64, now: i64) -> VideoId {
        let video = VideoId::from_bits(id);
        let upload = NewUpload {
            video,
            channel,
            title: "Waves",
            description: "",
            visibility: Visibility::Public,
            size: 1000,
            now,
        };
        store.create_upload(&upload).unwrap();
        video
    }

    #[test]
    fn an_upload_becomes_a_job_then_a_published_video() {
        let (store, owner, channel) = store();
        let video = upload(&store, channel, 7, 200);
        assert_eq!(store.owner_of(video).unwrap(), Some(owner));
        assert!(
            !store.finish_upload(video, 210).unwrap(),
            "bytes are missing"
        );
        store.set_received(video, 1000).unwrap();
        let state = store.studio_video(video).unwrap().unwrap().processing;
        assert_eq!(
            state,
            Processing::Uploading {
                received: 1000,
                size: 1000
            }
        );
        assert!(store.finish_upload(video, 220).unwrap());
        assert!(store.receiving(video).unwrap().is_none());
        assert!(
            store.recent_watchable(10).unwrap().is_empty(),
            "not yet playable"
        );

        let job = store.claim_job(230).unwrap().unwrap();
        assert_eq!(job.video, video);
        assert!(
            store.claim_job(230).unwrap().is_none(),
            "one worker per job"
        );
        store.job_progress(job.id, 400).unwrap();
        let state = store.studio_video(video).unwrap().unwrap().processing;
        assert_eq!(state, Processing::Processing { permille: 400 });

        let rendition = MadeRendition {
            width: 640,
            height: 360,
            bandwidth: 900_000,
            codecs: "avc1.64001e",
        };
        let made = Made {
            duration: MediaDuration {
                ticks: 140 * 3003,
                timescale: 90_000,
            },
            width: 1280,
            height: 720,
            renditions: &[rendition],
        };
        store.job_done(job, &made, 300).unwrap();
        let listed = store.recent_watchable(10).unwrap();
        assert_eq!(listed[0].card.published_at, 300);
        assert_eq!(listed[0].card.duration, made.duration);
    }

    #[test]
    fn a_job_interrupted_too_often_fails() {
        let (store, _, channel) = store();
        let video = upload(&store, channel, 8, 200);
        store.set_received(video, 1000).unwrap();
        store.finish_upload(video, 200).unwrap();
        for attempt in 1..=3 {
            assert!(store.claim_job(300).unwrap().is_some());
            let failed = store.recover_jobs(300).unwrap();
            assert_eq!(failed.is_empty(), attempt < 3, "attempt {attempt}");
        }
        assert!(store.claim_job(300).unwrap().is_none());
        let state = store.studio_video(video).unwrap().unwrap().processing;
        assert_eq!(
            state,
            Processing::Failed {
                reason: "internal".to_owned()
            }
        );
    }

    #[test]
    fn the_studio_pages_newest_first_and_edits_publish() {
        let (store, _, channel) = store();
        let older = upload(&store, channel, 9, 200);
        let newer = upload(&store, channel, 10, 300);
        let page = store.channel_videos(channel, None, 1).unwrap();
        assert_eq!(page[0].card.id, newer);
        let page = store.channel_videos(channel, Some(newer), 10).unwrap();
        assert_eq!(page.iter().map(|v| v.card.id).collect::<Vec<_>>(), [older]);
        let edit = VideoEdit {
            title: Some("Ripples"),
            visibility: Some(Visibility::Unlisted),
            ..VideoEdit::default()
        };
        store.edit_video(older, &edit, 400).unwrap();
        let edited = store.studio_video(older).unwrap().unwrap();
        assert_eq!(
            (edited.card.title.as_str(), edited.visibility),
            ("Ripples", Visibility::Unlisted)
        );
        store.delete_video(older).unwrap();
        assert!(store.studio_video(older).unwrap().is_none());
    }
}
