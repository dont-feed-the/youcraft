//! The job worker: makes finished uploads playable, one at a time, each in a child process.
//!
//! It claims the oldest queued job, runs `youcraft transcode` on the video's original into a
//! work directory, records the child's progress, and when the child reports what it made, moves
//! the directory to where media is served and marks the video ready. A child that fails, dies or
//! outlives the wall-clock limit fails its job, and the creator sees why in the studio, a child
//! the kernel stopped at one of its limits as such; the log keeps the detail. The site is
//! untouched. Jobs that were running when the server stopped are queued again at start.

use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, BufReader};
use youcraft_api::records::Failure;
use youcraft_store::jobs::{Job, JobDone, Made, MadeRendition};
use youcraft_transcode::Outcome;

use crate::site::{Site, now};
use crate::transcode::Report;

/// How often the worker looks for work when nothing wakes it.
const IDLE: std::time::Duration = std::time::Duration::from_secs(30);

/// Why a job failed: what its creator is told, and what the log says.
type Failed = (Failure, String);

/// Run jobs for as long as the server runs.
pub async fn serve(site: Arc<Site>) {
    recover(&site).await;
    loop {
        if !run_next(&site).await {
            let _ = tokio::time::timeout(IDLE, site.work.notified()).await;
        }
    }
}

/// Run jobs until none is queued, as `youcraft demo` does.
pub async fn drain(site: &Site) {
    recover(site).await;
    while run_next(site).await {}
}

async fn recover(site: &Site) {
    match site.db(move |store| store.recover_jobs(now())).await {
        Ok(failed) => {
            for video in failed {
                tracing::warn!("video {video}: its job was interrupted too often and failed");
            }
        }
        Err(_) => tracing::error!("jobs: could not recover the interrupted ones"),
    }
}

/// Run the next queued job; `false` when there is none.
async fn run_next(site: &Site) -> bool {
    let Ok(Some(job)) = site.db(move |store| store.claim_job(now())).await else {
        return false;
    };
    tracing::info!("video {}: making renditions", job.video);
    let config = site.config();
    let work = config.work().join(job.video.to_string());
    let _ = tokio::fs::remove_dir_all(&work).await;
    let result = match tokio::fs::create_dir_all(&work).await {
        Ok(()) => {
            transcode(
                site,
                job,
                &config.originals().join(job.video.to_string()),
                &work,
            )
            .await
        }
        Err(e) => Err((Failure::Internal, format!("{}: {e}", work.display()))),
    };
    let finished = match result {
        Ok(outcome) => publish(site, job, &outcome, &work).await,
        Err(failure) => Err(failure),
    };
    if let Err((failure, detail)) = finished {
        tracing::warn!("video {}: {}: {detail}", job.video, failure.as_str());
        let _ = tokio::fs::remove_dir_all(&work).await;
        if site
            .db(move |store| store.job_failed(job, failure, now()))
            .await
            .is_err()
        {
            tracing::error!("video {}: could not record that its job failed", job.video);
        }
    }
    true
}

/// What the end of a child that said nothing of its own failure means: a signal from one of
/// the limits it set on itself (processor time, file size, address space, which aborts on a
/// failed allocation) or from the kernel's memory killer is a limit; anything else, a decoder
/// that died on the file.
fn stopped(status: ExitStatus) -> Failed {
    let limit = match status.signal() {
        Some(24) => Some("SIGXCPU: out of processor time"),
        Some(25) => Some("SIGXFSZ: a file grew too large"),
        Some(6) => Some("SIGABRT: out of memory"),
        Some(9) => Some("SIGKILL: killed, out of memory"),
        _ => None,
    };
    match limit {
        Some(why) => (Failure::Limits, format!("the transcoder stopped: {why}")),
        None => (Failure::Decode, format!("the transcoder stopped: {status}")),
    }
}

/// Run the child on `input` into `work` and follow its reports.
async fn transcode(site: &Site, job: Job, input: &Path, work: &Path) -> Result<Outcome, Failed> {
    let internal = |detail: String| (Failure::Internal, detail);
    let limits = site.config().transcode;
    let program = std::env::current_exe().map_err(|e| internal(format!("own path: {e}")))?;
    let mut child = tokio::process::Command::new(program)
        .arg("transcode")
        .arg(input)
        .arg(work)
        .arg(limits.memory_bytes.to_string())
        .arg(limits.longest_video.as_secs().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| internal(format!("start: {e}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| internal("no output".to_owned()))?;
    let follow = async {
        let mut lines = BufReader::new(stdout).lines();
        let mut last = None;
        while let Ok(Some(line)) = lines.next_line().await {
            match Report::parse(&line) {
                Some(Report::Progress(permille)) => {
                    let _ = site
                        .db(move |store| store.job_progress(job.id, permille))
                        .await;
                }
                Some(report) => last = Some(report),
                None => {}
            }
        }
        let status = child.wait().await;
        (last, status)
    };
    match tokio::time::timeout(limits.wall, follow).await {
        Err(_) => Err((
            Failure::Limits,
            format!("took longer than {} s", limits.wall.as_secs()),
        )),
        Ok((Some(Report::Outcome(outcome)), Ok(status))) if status.success() => Ok(*outcome),
        Ok((Some(Report::Failure(failure, detail)), _)) => Err((failure, detail)),
        Ok((_, Ok(status))) => Err(stopped(status)),
        Ok((_, Err(e))) => Err(internal(format!("the transcoder vanished: {e}"))),
    }
}

/// Serve what the child made, and mark the video ready. If the video went while it was made,
/// its media goes too; if the database refuses, the media goes and the job fails.
async fn publish(site: &Site, job: Job, outcome: &Outcome, work: &Path) -> Result<(), Failed> {
    let internal = |detail: String| (Failure::Internal, detail);
    let media = site.config().media().join(job.video.to_string());
    let _ = tokio::fs::remove_dir_all(&media).await;
    tokio::fs::rename(work, &media)
        .await
        .map_err(|e| internal(format!("{}: {e}", media.display())))?;
    let outcome = outcome.clone();
    let done = site
        .db(move |store| {
            let renditions: Vec<MadeRendition<'_>> = outcome
                .renditions
                .iter()
                .map(|r| MadeRendition {
                    width: r.width,
                    height: r.height,
                    bandwidth: r.bandwidth,
                    codecs: &r.codecs,
                })
                .collect();
            let made = Made {
                duration: outcome.duration,
                width: outcome.width,
                height: outcome.height,
                renditions: &renditions,
            };
            store.job_done(job, &made, now())
        })
        .await;
    match done {
        Ok(JobDone::Ready) => {
            tracing::info!("video {}: ready", job.video);
            Ok(())
        }
        Ok(JobDone::Gone) => {
            tracing::info!("video {}: deleted while it was being made", job.video);
            remove_media(job, &media).await;
            Ok(())
        }
        // the database's error is in the log already
        Err(_) => {
            remove_media(job, &media).await;
            Err(internal("could not mark the video ready".to_owned()))
        }
    }
}

/// Remove the media directory `media` of `job`'s video, saying so when it stays.
async fn remove_media(job: Job, media: &Path) {
    if let Err(e) = tokio::fs::remove_dir_all(media).await {
        tracing::warn!("video {}: its media stays: {e}", job.video);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_child_stopped_at_a_limit_says_so() {
        for signal in [24, 25, 6, 9] {
            assert_eq!(
                stopped(ExitStatus::from_raw(signal)).0,
                Failure::Limits,
                "{signal}"
            );
        }
        // a segmentation fault, or an exit code of its own, is the decoder dying on the file
        assert_eq!(stopped(ExitStatus::from_raw(11)).0, Failure::Decode);
        assert_eq!(stopped(ExitStatus::from_raw(101 << 8)).0, Failure::Decode);
    }
}
