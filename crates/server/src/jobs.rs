//! The job worker: makes finished uploads playable, one at a time, each in a child process.
//!
//! It claims the oldest queued job, runs `youcraft transcode` on the video's original into a
//! work directory, records the child's progress, and when the child reports what it made, moves
//! the directory to where media is served and marks the video ready. A child that fails, dies or
//! outlives the wall-clock limit fails its job, and the creator sees why in the studio; the site
//! is untouched. Jobs that were running when the server stopped are queued again at start.

use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, BufReader};
use youcraft_store::jobs::{Job, Made, MadeRendition};
use youcraft_transcode::Outcome;

use crate::site::{Site, now};
use crate::transcode::Report;

/// How often the worker looks for work when nothing wakes it.
const IDLE: std::time::Duration = std::time::Duration::from_secs(30);

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
        Err(e) => Err(("internal".to_owned(), format!("{}: {e}", work.display()))),
    };
    let finished = match result {
        Ok(outcome) => publish(site, job, &outcome, &work).await,
        Err(failure) => Err(failure),
    };
    if let Err((kind, detail)) = finished {
        tracing::warn!("video {}: {kind}: {detail}", job.video);
        let _ = tokio::fs::remove_dir_all(&work).await;
        let _ = site
            .db(move |store| store.job_failed(job, &kind, now()))
            .await;
    }
    true
}

/// Run the child on `input` into `work` and follow its reports.
async fn transcode(
    site: &Site,
    job: Job,
    input: &Path,
    work: &Path,
) -> Result<Outcome, (String, String)> {
    let internal = |detail: String| ("internal".to_owned(), detail);
    let program = std::env::current_exe().map_err(|e| internal(format!("own path: {e}")))?;
    let mut child = tokio::process::Command::new(program)
        .arg("transcode")
        .arg(input)
        .arg(work)
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
    match tokio::time::timeout(site.config().transcode.wall, follow).await {
        Err(_) => Err(internal("took longer than allowed".to_owned())),
        Ok((Some(Report::Outcome(outcome)), Ok(status))) if status.success() => Ok(*outcome),
        Ok((Some(Report::Failure(kind)), _)) => {
            Err((kind, "the transcoder refused the file".to_owned()))
        }
        Ok((_, status)) => Err((
            "decode".to_owned(),
            format!("the transcoder stopped: {status:?}"),
        )),
    }
}

/// Serve what the child made, and mark the video ready.
async fn publish(
    site: &Site,
    job: Job,
    outcome: &Outcome,
    work: &Path,
) -> Result<(), (String, String)> {
    let internal = |detail: String| ("internal".to_owned(), detail);
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
        Ok(()) => {
            tracing::info!("video {}: ready", job.video);
            Ok(())
        }
        // the video was deleted while it was being made
        Err(_) => {
            let _ = tokio::fs::remove_dir_all(&media).await;
            Err(internal("the video is gone".to_owned()))
        }
    }
}
