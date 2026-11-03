//! The jobs that make uploads playable: claimed one at a time, then done or failed.
//!
//! A job that was running when the server stopped is queued again when it starts, up to
//! [`MAX_ATTEMPTS`] times; a job that keeps killing its worker then fails instead.

use rusqlite::{OptionalExtension, params};
use youcraft_api::VideoId;
use youcraft_api::time::MediaDuration;

use crate::{Store, StoreError};

/// How many times a job is started before it fails for good.
pub const MAX_ATTEMPTS: i64 = 3;

/// A job claimed by the worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Job {
    /// Its identifier.
    pub id: i64,
    /// The video it makes playable.
    pub video: VideoId,
}

/// One rendition a job made.
pub struct MadeRendition<'a> {
    /// Its size in pixels.
    pub width: u32,
    /// Its size in pixels.
    pub height: u32,
    /// Bits per second of its busiest segment.
    pub bandwidth: u64,
    /// Its RFC 6381 codecs.
    pub codecs: &'a str,
}

/// What a job made of its video.
pub struct Made<'a> {
    /// How long the video lasts.
    pub duration: MediaDuration,
    /// The picture's size as shown.
    pub width: u32,
    /// The picture's size as shown.
    pub height: u32,
    /// Its renditions.
    pub renditions: &'a [MadeRendition<'a>],
}

impl Store {
    /// Queue again the jobs that were running when the server stopped, failing those started
    /// too often; answers the videos that failed.
    pub fn recover_jobs(&self, now: i64) -> Result<Vec<VideoId>, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let failed: Vec<i64> = transaction
            .prepare("SELECT video_id FROM jobs WHERE state = 'running' AND attempts >= ?1")?
            .query_map([MAX_ATTEMPTS], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        for &video in &failed {
            transaction.execute(
                "UPDATE videos SET state = 'failed', failure = 'internal' WHERE id = ?1",
                [video],
            )?;
        }
        transaction.execute(
            "UPDATE jobs SET state = 'failed', failure = 'internal', finished_at = ?2 \
             WHERE state = 'running' AND attempts >= ?1",
            params![MAX_ATTEMPTS, now],
        )?;
        transaction.execute(
            "UPDATE jobs SET state = 'queued' WHERE state = 'running'",
            [],
        )?;
        transaction.commit()?;
        Ok(failed.into_iter().map(VideoId::from_i64).collect())
    }

    /// Claim the oldest queued job.
    pub fn claim_job(&self, now: i64) -> Result<Option<Job>, StoreError> {
        let job = self
            .connection()
            .query_row(
                "UPDATE jobs SET state = 'running', attempts = attempts + 1, started_at = ?1, progress_permille = 0 \
                 WHERE id = (SELECT id FROM jobs WHERE state = 'queued' ORDER BY created_at, id LIMIT 1) \
                 RETURNING id, video_id",
                [now],
                |row| Ok(Job { id: row.get(0)?, video: VideoId::from_i64(row.get(1)?) }),
            )
            .optional()?;
        Ok(job)
    }

    /// Record how far `job` is, in thousandths.
    pub fn job_progress(&self, job: i64, permille: u32) -> Result<(), StoreError> {
        self.connection().execute(
            "UPDATE jobs SET progress_permille = ?2 WHERE id = ?1",
            params![job, permille.min(1000)],
        )?;
        Ok(())
    }

    /// `job` made its video playable as `made`; a public video is published now.
    pub fn job_done(&self, job: Job, made: &Made<'_>, now: i64) -> Result<(), StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE jobs SET state = 'done', progress_permille = 1000, finished_at = ?2 WHERE id = ?1",
            params![job.id, now],
        )?;
        let video = job.video.to_i64();
        transaction.execute("DELETE FROM renditions WHERE video_id = ?1", [video])?;
        for rendition in made.renditions {
            transaction.execute(
                "INSERT INTO renditions (video_id, height, width, bandwidth, codecs) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    video, rendition.height, rendition.width,
                    i64::try_from(rendition.bandwidth).unwrap_or(i64::MAX), rendition.codecs
                ],
            )?;
        }
        transaction.execute(
            "UPDATE videos SET state = 'ready', failure = NULL, duration_ticks = ?2, timescale = ?3, width = ?4, height = ?5, \
             published_at = CASE WHEN visibility = 'public' THEN COALESCE(published_at, ?6) ELSE published_at END \
             WHERE id = ?1",
            params![
                video, i64::try_from(made.duration.ticks).unwrap_or(i64::MAX), made.duration.timescale.max(1),
                made.width, made.height, now
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// `job` could not make its video playable, for `reason`.
    pub fn job_failed(&self, job: Job, reason: &str, now: i64) -> Result<(), StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE jobs SET state = 'failed', failure = ?2, finished_at = ?3 WHERE id = ?1",
            params![job.id, reason, now],
        )?;
        transaction.execute(
            "UPDATE videos SET state = 'failed', failure = ?2 WHERE id = ?1",
            params![job.video.to_i64(), reason],
        )?;
        transaction.commit()?;
        Ok(())
    }
}
