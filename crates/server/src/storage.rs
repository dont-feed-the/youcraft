//! The files of the site's videos: how much of its storage they fill, what a new upload may
//! claim of it, and their removal.
//!
//! The database counts what is stored (see `youcraft_store::uploads`): each video's files, as
//! measured here once after each change, and the bytes uploads have received and have yet to
//! receive. Measuring a video walks its own few files; the transcodes in progress, which no row
//! counts, are walked too. Nothing walks every video's files.

use std::path::{Path, PathBuf};

use youcraft_api::{ApiError, VideoId};
use youcraft_store::uploads::Allowance;

use crate::config::Config;
use crate::site::Site;
use crate::uploads::lapsing;

/// The deepest a video's files lie below the directory that holds them all.
const DEPTH: usize = 4;
/// How many videos one database read gives to measure.
const BATCH: u32 = 256;
/// The most batches one allowance measures; what is left waits for the next upload.
const BATCHES: u32 = 64;

/// What an upload starting now may claim.
pub async fn allowance(site: &Site) -> Result<Allowance, ApiError> {
    let limits = site.config().uploads;
    let unrecorded = if limits.storage_bytes == u64::MAX {
        0
    } else {
        measure(site).await?;
        let work = site.config().work();
        walk(move || bytes_under(&work)).await?
    };
    Ok(Allowance {
        storage: limits.storage_bytes,
        unrecorded,
        file: limits.file_bytes,
        open: limits.open,
        open_bytes: limits.open_bytes,
        since: lapsing(site),
    })
}

/// Measure the files of every video that changed since it was last measured.
async fn measure(site: &Site) -> Result<(), ApiError> {
    for _ in 0..BATCHES {
        let videos = site.db(|store| store.unmeasured(BATCH)).await?;
        if videos.is_empty() {
            break;
        }
        for video in videos {
            let config = site.config().clone();
            let bytes = walk(move || stored(&config, video)).await?;
            site.db(move |store| store.set_stored_bytes(video, bytes))
                .await?;
        }
    }
    Ok(())
}

/// Run `count` off the async threads.
async fn walk(count: impl FnOnce() -> u64 + Send + 'static) -> Result<u64, ApiError> {
    tokio::task::spawn_blocking(count).await.map_err(|e| {
        tracing::error!("storage: {e}");
        ApiError::Internal
    })
}

/// The bytes the files of `video` fill: its original and its media.
fn stored(config: &Config, video: VideoId) -> u64 {
    let name = video.to_string();
    let original = std::fs::symlink_metadata(config.originals().join(&name))
        .map_or(0, |metadata| metadata.len());
    original.saturating_add(bytes_under(&config.media().join(name)))
}

/// The bytes of the files under `root`, to [`DEPTH`] levels; what cannot be read counts nothing.
fn bytes_under(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut pending: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() && depth < DEPTH {
                pending.push((entry.path(), depth.saturating_add(1)));
            } else if kind.is_file()
                && let Ok(metadata) = entry.metadata()
            {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    total
}

/// Remove every file of `video`: its upload, original, media and transcode in progress.
pub async fn remove_files_of(config: &Config, video: VideoId) {
    let name = video.to_string();
    for path in [
        config.uploads().join(&name),
        config.originals().join(&name),
        config.media().join(&name),
        config.work().join(&name),
    ] {
        remove(&path).await;
    }
}

/// Remove the file or directory at `path`, if there is one; a failure is logged.
pub async fn remove(path: &Path) {
    let removed = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) if metadata.is_dir() => tokio::fs::remove_dir_all(path).await,
        Ok(_) => tokio::fs::remove_file(path).await,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    };
    if let Err(e) = removed {
        tracing::error!("{}: {e}", path.display());
    }
}
