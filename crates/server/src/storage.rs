//! How much of the site's storage its videos fill, and how much room an upload may still claim.
//!
//! The videos fill their originals, renditions, transcodes in progress and the bytes uploads
//! have received, all on disk; an upload also claims, from when it starts, the bytes it has yet
//! to receive, so that two uploads cannot both count on the same room. A claim lapses a day after
//! its upload started, so that one abandoned upload cannot hold the site's room for ever.

use std::path::{Path, PathBuf};

use youcraft_api::ApiError;

use crate::config::Config;
use crate::site::{Site, now};

/// How long an upload's claim on the room it has yet to fill lasts, in seconds.
const CLAIM: i64 = 24 * 60 * 60;

/// The deepest a video's files lie below the directory that holds them all.
const DEPTH: usize = 4;

/// The bytes the site may still take in: its storage limit, less what its videos fill and
/// what uploads in progress claim.
pub async fn room(site: &Site) -> Result<u64, ApiError> {
    let limit = site.config().storage_bytes;
    if limit == u64::MAX {
        return Ok(limit);
    }
    let config = site.config().clone();
    let filled = tokio::task::spawn_blocking(move || filled(&config))
        .await
        .map_err(|e| {
            tracing::error!("storage: {e}");
            ApiError::Internal
        })?;
    let promised = site
        .db(|store| store.promised(now().saturating_sub(CLAIM)))
        .await?;
    Ok(limit.saturating_sub(filled.saturating_add(promised)))
}

/// The bytes on disk under the directories that hold videos.
fn filled(config: &Config) -> u64 {
    [
        config.uploads(),
        config.originals(),
        config.work(),
        config.media(),
    ]
    .iter()
    .map(|dir| bytes_under(dir))
    .fold(0, u64::saturating_add)
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
