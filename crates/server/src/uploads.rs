//! The bytes of uploads: `PATCH /upload/<video>` appends a piece at its offset, `HEAD` answers
//! the offset to resume from, as the tus protocol does; and the sweep that drops uploads that
//! lapsed.
//!
//! A piece is accepted only from the video's channel (or an administrator), before its upload
//! lapses, at the exact offset received so far; all of that is checked before a byte of the body
//! is read, and the body is then streamed to the file, never held whole, and cut off past what
//! the piece may carry. A client that lost an answer asks again and resends; the file is cut back
//! to that offset before each write, so bytes that reached the disk but not the database are
//! written again, never twice. Pieces of one upload never interleave, a piece that stalls is
//! dropped, and the site receives at most so many at once.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use http_body_util::BodyExt;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::time::Instant;
use youcraft_api::commands::upload::Create;
use youcraft_api::{ApiError, Command, Rule, VideoId};
use youcraft_store::uploads::Receiving;

use crate::handlers::studio::may_manage;
use crate::handlers::upload::PIECE;
use crate::http::error_response;
use crate::site::{Caller, Site, now};
use crate::storage;

/// The header that carries an offset, both ways.
pub const OFFSET: &str = "upload-offset";
/// The header that carries the size of the whole file.
pub const LENGTH: &str = "upload-length";

/// The longest a piece may go without a byte before it is dropped.
const IDLE: Duration = Duration::from_secs(60);
/// The slowest a piece may arrive, in bytes a second, after [`GRACE`].
const SLOWEST: u64 = 16 << 10;
/// The time a piece has on top of its bytes at [`SLOWEST`].
const GRACE: Duration = Duration::from_secs(30);
/// How often the sweep looks for uploads that lapsed.
const SWEEP: Duration = Duration::from_secs(10 * 60);
/// The most lapsed uploads one sweep drops; the next one drops the rest.
const SWEPT: u32 = 256;

fn with_offset(status: StatusCode, offset: u64, length: Option<u64>) -> Response {
    let mut response = status.into_response();
    let headers = response.headers_mut();
    headers.insert(OFFSET, HeaderValue::from(offset));
    if let Some(length) = length {
        headers.insert(LENGTH, HeaderValue::from(length));
    }
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

/// When the uploads that started before lapse, in seconds since 1970.
pub fn lapsing(site: &Site) -> i64 {
    let lifetime = site.config().uploads.lifetime.as_secs();
    now().saturating_sub(i64::try_from(lifetime).unwrap_or(i64::MAX))
}

/// The upload of `video` that `caller` may send to: of their channel, or any if they run the
/// site, and not lapsed.
pub async fn open_upload(
    site: &Site,
    caller: Caller,
    video: VideoId,
) -> Result<Receiving, ApiError> {
    if !caller.role.allows(Create::REQUIRES) {
        return Err(if caller.account.is_none() {
            ApiError::SignInRequired
        } else {
            ApiError::Forbidden
        });
    }
    may_manage(site, caller, video).await?;
    let receiving = site
        .db(move |store| store.receiving(video))
        .await?
        .ok_or(ApiError::Conflict)?;
    if receiving.created_at < lapsing(site) {
        return Err(ApiError::NotFound);
    }
    Ok(receiving)
}

/// `HEAD /upload/<video>`: how many bytes have arrived, of how many.
pub async fn offset(site: &Site, caller: Caller, video: &str) -> Response {
    let Ok(video) = video.parse::<VideoId>() else {
        return error_response(&ApiError::NotFound);
    };
    match open_upload(site, caller, video).await {
        Ok(receiving) => with_offset(StatusCode::OK, receiving.received, Some(receiving.size)),
        Err(error) => error_response(&error),
    }
}

/// Why a piece did not reach the file.
enum Lost {
    /// It carried more than it may.
    TooLarge,
    /// It stopped arriving, or arrived too slowly.
    Stalled,
    /// The client broke it off.
    Broken,
    /// The disk failed.
    Disk(std::io::Error),
}

/// `PATCH /upload/<video>`: append `body` at the offset `headers` name.
pub async fn receive(
    site: &Site,
    caller: Caller,
    video: &str,
    headers: &HeaderMap,
    body: Body,
) -> Response {
    let Some(offset) = headers
        .get(OFFSET)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
    else {
        return error_response(&ApiError::invalid(OFFSET, Rule::Malformed));
    };
    let Ok(video) = video.parse::<VideoId>() else {
        return error_response(&ApiError::NotFound);
    };
    let _held = match site.lock_upload(video).await {
        Ok(held) => held,
        Err(error) => return error_response(&error),
    };
    let receiving = match open_upload(site, caller, video).await {
        Ok(receiving) => receiving,
        Err(error) => return error_response(&error),
    };
    if offset != receiving.received {
        return with_offset(
            StatusCode::CONFLICT,
            receiving.received,
            Some(receiving.size),
        );
    }
    let most = PIECE.min(receiving.size.saturating_sub(offset));
    let declared = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    if declared.is_some_and(|length| length > most) {
        return error_response(&ApiError::TooLarge { limit: most });
    }
    let Ok(_stream) = site.streams.acquire().await else {
        return error_response(&ApiError::Internal);
    };
    let path = site.config().uploads().join(video.to_string());
    let length = match write_piece(&path, offset, body, most).await {
        Ok(length) => length,
        Err(Lost::TooLarge) => return error_response(&ApiError::TooLarge { limit: most }),
        Err(Lost::Stalled) => return with_offset(StatusCode::REQUEST_TIMEOUT, offset, None),
        Err(Lost::Broken) => return with_offset(StatusCode::BAD_REQUEST, offset, None),
        Err(Lost::Disk(e)) => {
            tracing::error!("upload {video}: {e}");
            return error_response(&ApiError::Internal);
        }
    };
    let received = offset.saturating_add(length);
    match site
        .db(move |store| store.set_received(video, received))
        .await
    {
        Ok(()) => with_offset(StatusCode::NO_CONTENT, received, None),
        Err(error) => error_response(&error),
    }
}

/// Write `body` to the file at `path` from `offset`, cut back to it first; at most `most`
/// bytes, each within [`IDLE`] of the last and all within the time [`SLOWEST`] allows. The bytes
/// written, synced to the disk.
async fn write_piece(path: &Path, offset: u64, mut body: Body, most: u64) -> Result<u64, Lost> {
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
        .await
        .map_err(Lost::Disk)?;
    file.set_len(offset).await.map_err(Lost::Disk)?;
    file.seek(std::io::SeekFrom::End(0))
        .await
        .map_err(Lost::Disk)?;
    let allowed = GRACE.saturating_add(Duration::from_secs(most / SLOWEST));
    let deadline = Instant::now().checked_add(allowed);
    let mut length = 0u64;
    loop {
        let idle = Instant::now().checked_add(IDLE);
        let until = match (deadline, idle) {
            (Some(deadline), Some(idle)) => deadline.min(idle),
            (one, other) => one.or(other).ok_or(Lost::Stalled)?,
        };
        let frame = match tokio::time::timeout_at(until, body.frame()).await {
            Err(_) => return Err(Lost::Stalled),
            Ok(None) => break,
            Ok(Some(Err(_))) => return Err(Lost::Broken),
            Ok(Some(Ok(frame))) => frame,
        };
        let Ok(bytes) = frame.into_data() else {
            continue;
        };
        length = length.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        if length > most {
            return Err(Lost::TooLarge);
        }
        file.write_all(&bytes).await.map_err(Lost::Disk)?;
    }
    file.sync_data().await.map_err(Lost::Disk)?;
    Ok(length)
}

/// Drop the uploads that lapsed, now and every [`SWEEP`], for as long as the server runs: their
/// videos, their rows and the bytes they received, which frees what they claimed. Files of
/// uploads that no row knows any more go too.
pub async fn sweep(site: Arc<Site>) {
    loop {
        sweep_once(&site).await;
        tokio::time::sleep(SWEEP).await;
    }
}

/// One sweep.
async fn sweep_once(site: &Site) {
    let since = lapsing(site);
    let lapsed = site
        .db(move |store| store.lapsed_uploads(since, SWEPT))
        .await
        .unwrap_or_default();
    for video in lapsed {
        let Ok(_held) = site.lock_upload(video).await else {
            continue;
        };
        if let Ok(true) = site
            .db(move |store| store.drop_lapsed_upload(video, since))
            .await
        {
            tracing::info!("upload {video}: lapsed, dropped");
            storage::remove(&site.config().uploads().join(video.to_string())).await;
        }
    }
    let Ok(mut files) = tokio::fs::read_dir(site.config().uploads()).await else {
        return;
    };
    while let Ok(Some(entry)) = files.next_entry().await {
        let Some(video) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<VideoId>().ok())
        else {
            continue;
        };
        let Ok(_held) = site.lock_upload(video).await else {
            continue;
        };
        if let Ok(None) = site.db(move |store| store.receiving(video)).await {
            tracing::info!("upload {video}: no upload holds its file, removed");
            storage::remove(&entry.path()).await;
        }
    }
}

#[cfg(test)]
pub mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};

    use axum::body::Bytes;
    use axum::http::HeaderMap;
    use youcraft_api::{AccountId, ChannelId, Grant, Role};
    use youcraft_store::accounts::NewAccount;

    use super::*;
    use crate::config::Config;
    use crate::site::{AddressKey, Credential, ViewerKey};

    /// A site in a fresh data directory, with a storage limit of 2 MiB, which the caller removes.
    pub fn site(name: &str) -> (Site, PathBuf) {
        site_with(name, |_| {})
    }

    /// [`site`], with its configuration changed by `change`.
    pub fn site_with(name: &str, change: impl FnOnce(&mut Config)) -> (Site, PathBuf) {
        let data = std::env::temp_dir().join(format!("youcraft-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let mut config = Config::testing(data.clone());
        config.uploads.storage_bytes = 2 << 20;
        config.uploads.open = 8;
        config.uploads.open_bytes = u64::MAX;
        change(&mut config);
        (Site::open(&config).unwrap(), data)
    }

    /// The creator of account `bits`, with a channel.
    fn account(site: &Site, bits: u64) -> Caller {
        let account = AccountId::from_bits(bits);
        site.store()
            .create_account(&NewAccount {
                id: account,
                channel: ChannelId::from_bits(bits + 1),
                google_subject: &format!("g{bits}"),
                email: &format!("{bits}@example.com"),
                name: "Ada",
                handle: &format!("ada{bits}"),
                role: Grant::Creator,
                now: 0,
            })
            .unwrap();
        Caller {
            account: Some(account),
            role: Role::Creator,
            credential: Credential::Token,
            viewer: ViewerKey::NOBODY,
            address: AddressKey::NOBODY,
        }
    }

    /// `caller` starts uploading a file of `size` bytes.
    async fn start(site: &Site, caller: Caller, size: u64) -> Result<VideoId, ApiError> {
        let input = serde_json::json!({ "title": "Waves", "size": size });
        let upload = crate::dispatch::run(site, caller, "upload.create", input).await?;
        Ok(upload["video"].as_str().unwrap().parse().unwrap())
    }

    /// A creator, and a private video they started uploading: 10 bytes.
    pub async fn creator(site: &Site) -> (Caller, VideoId) {
        let caller = account(site, 1);
        (caller, start(site, caller, 10).await.unwrap())
    }

    fn at(offset: u64) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(OFFSET, HeaderValue::from(offset));
        headers
    }

    fn offset_of(response: &Response) -> u64 {
        response.headers()[OFFSET]
            .to_str()
            .unwrap()
            .parse()
            .unwrap()
    }

    fn bytes(piece: &'static [u8]) -> Body {
        Body::from(Bytes::from_static(piece))
    }

    async fn finish(
        site: &Site,
        caller: Caller,
        video: VideoId,
    ) -> Result<serde_json::Value, ApiError> {
        let which = serde_json::json!({ "video": video });
        crate::dispatch::run(site, caller, "upload.finish", which).await
    }

    #[tokio::test]
    async fn pieces_arrive_at_their_offset_once_and_become_the_original() {
        let (site, dir) = site("uploads");
        let (owner, video) = creator(&site).await;
        let name = video.to_string();
        let first = receive(&site, owner, &name, &at(0), bytes(b"0123")).await;
        assert_eq!(
            (first.status(), offset_of(&first)),
            (StatusCode::NO_CONTENT, 4)
        );
        let again = receive(&site, owner, &name, &at(0), bytes(b"0123")).await;
        assert_eq!(
            (again.status(), offset_of(&again)),
            (StatusCode::CONFLICT, 4),
            "a resent piece is refused"
        );
        let stranger = Caller {
            account: Some(youcraft_api::AccountId::from_bits(9)),
            ..owner
        };
        let theirs = receive(&site, stranger, &name, &at(4), bytes(b"x")).await;
        assert_eq!(theirs.status(), StatusCode::NOT_FOUND);
        let beyond = receive(&site, owner, &name, &at(4), bytes(b"4567890")).await;
        assert_eq!(beyond.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let early = finish(&site, owner, video).await;
        assert_eq!(early, Err(ApiError::Conflict), "bytes are missing");
        let rest = receive(&site, owner, &name, &at(4), bytes(b"456789")).await;
        assert_eq!(offset_of(&rest), 10);
        let asked = offset(&site, owner, &name).await;
        assert_eq!((asked.status(), offset_of(&asked)), (StatusCode::OK, 10));
        finish(&site, owner, video).await.unwrap();
        let original = std::fs::read(site.config().originals().join(&name)).unwrap();
        assert_eq!(original, b"0123456789");
        let job = site.store().claim_job(0).unwrap().unwrap();
        assert_eq!(job.video, video);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A body that remembers whether anyone read it.
    fn watched(read: &Arc<AtomicBool>) -> Body {
        let read = Arc::clone(read);
        Body::from_stream(futures_util::stream::once(async move {
            read.store(true, Ordering::SeqCst);
            Ok::<_, std::io::Error>(Bytes::from_static(b"x"))
        }))
    }

    #[tokio::test]
    async fn a_piece_is_refused_before_a_byte_of_it_is_read() {
        let (site, dir) = site("unread");
        let (owner, video) = creator(&site).await;
        let name = video.to_string();
        let stranger = Caller {
            account: Some(AccountId::from_bits(9)),
            ..owner
        };
        for (caller, offset, status) in [
            (Caller::ANONYMOUS, 0, StatusCode::UNAUTHORIZED),
            (stranger, 0, StatusCode::NOT_FOUND),
            (owner, 3, StatusCode::CONFLICT),
        ] {
            let read = Arc::new(AtomicBool::new(false));
            let answer = receive(&site, caller, &name, &at(offset), watched(&read)).await;
            assert_eq!(answer.status(), status);
            assert!(
                !read.load(Ordering::SeqCst),
                "{status}: the body stays unread"
            );
        }
        let mut declared = at(0);
        declared.insert(header::CONTENT_LENGTH, HeaderValue::from(11));
        let read = Arc::new(AtomicBool::new(false));
        let answer = receive(&site, owner, &name, &declared, watched(&read)).await;
        assert_eq!(answer.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert!(
            !read.load(Ordering::SeqCst),
            "more than is left, by its length"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_piece_longer_than_it_says_is_cut_off() {
        let (site, dir) = site("cut-off");
        let (owner, video) = creator(&site).await;
        let name = video.to_string();
        let frames: Vec<Result<Bytes, std::io::Error>> = vec![
            Ok(Bytes::from_static(b"012345")),
            Ok(Bytes::from_static(b"6789X")),
        ];
        let body = Body::from_stream(futures_util::stream::iter(frames));
        let answer = receive(&site, owner, &name, &at(0), body).await;
        assert_eq!(answer.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let asked = offset(&site, owner, &name).await;
        assert_eq!(offset_of(&asked), 0, "nothing of it counts");
        let whole = receive(&site, owner, &name, &at(0), bytes(b"0123456789")).await;
        assert_eq!(offset_of(&whole), 10);
        let file = std::fs::read(site.config().uploads().join(&name)).unwrap();
        assert_eq!(
            file, b"0123456789",
            "the file is cut back before it is written"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn uploads_share_the_sites_room_and_claim_it_from_their_start() {
        let (site, dir) = site("room");
        let (owner, video) = creator(&site).await;
        let transcoding = site.config().work().join("elsewhere");
        std::fs::create_dir_all(&transcoding).unwrap();
        std::fs::write(transcoding.join("filled"), vec![0u8; 1 << 19]).unwrap();
        let claimed = 10 + (1 << 19);
        let first = start(&site, owner, 1 << 20).await;
        assert!(first.is_ok(), "{first:?}");
        let refused = start(&site, owner, 1 << 20).await;
        let room = (2 << 20) - claimed - (1 << 20);
        assert_eq!(refused, Err(ApiError::TooLarge { limit: room }));
        let name = video.to_string();
        receive(&site, owner, &name, &at(0), bytes(b"0123")).await;
        assert!(
            start(&site, owner, room).await.is_ok(),
            "received bytes move from claimed to filled"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_finished_upload_fills_what_its_files_measure() {
        let (site, dir) = site_with("measured", |config| config.uploads.storage_bytes = 100);
        let (owner, video) = creator(&site).await;
        receive(
            &site,
            owner,
            &video.to_string(),
            &at(0),
            bytes(b"0123456789"),
        )
        .await;
        finish(&site, owner, video).await.unwrap();
        let media = site.config().media().join(video.to_string());
        std::fs::create_dir_all(media.join("360p")).unwrap();
        std::fs::write(media.join("360p").join("video.mp4"), [0u8; 30]).unwrap();
        assert_eq!(
            start(&site, owner, 100).await,
            Err(ApiError::TooLarge { limit: 60 }),
            "the original and the media it has when measured"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn an_account_keeps_only_so_many_uploads_open() {
        let (site, dir) = site_with("open", |config| {
            config.uploads.open = 2;
            config.uploads.open_bytes = 100;
        });
        let ada = account(&site, 1);
        assert!(start(&site, ada, 60).await.is_ok());
        assert_eq!(
            start(&site, ada, 41).await,
            Err(ApiError::TooLarge { limit: 40 })
        );
        assert!(start(&site, ada, 40).await.is_ok());
        assert_eq!(start(&site, ada, 1).await, Err(ApiError::Conflict));
        let bob = account(&site, 3);
        assert!(start(&site, bob, 100).await.is_ok(), "each account its own");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_lapsed_upload_takes_nothing_more_and_is_swept() {
        let (site, dir) = site_with("lapsed", |config| {
            config.uploads.lifetime = Duration::ZERO;
        });
        let (owner, video) = creator(&site).await;
        let name = video.to_string();
        let file = site.config().uploads().join(&name);
        std::fs::write(&file, b"0123").unwrap();
        let orphan = site
            .config()
            .uploads()
            .join(VideoId::from_bits(77).to_string());
        std::fs::write(&orphan, b"lost").unwrap();
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let late = receive(&site, owner, &name, &at(0), bytes(b"0123")).await;
        assert_eq!(late.status(), StatusCode::NOT_FOUND);
        assert_eq!(finish(&site, owner, video).await, Err(ApiError::NotFound));
        sweep_once(&site).await;
        assert!(
            !file.exists() && !orphan.exists(),
            "its bytes and an orphan's go"
        );
        assert_eq!(site.store().owner_of(video).unwrap(), None, "and its video");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn finishing_again_after_a_crash_finds_the_file_moved() {
        let (site, dir) = site("crash");
        let (owner, video) = creator(&site).await;
        let name = video.to_string();
        receive(&site, owner, &name, &at(0), bytes(b"0123456789")).await;
        std::fs::rename(
            site.config().uploads().join(&name),
            site.config().originals().join(&name),
        )
        .unwrap();
        assert!(finish(&site, owner, video).await.is_ok());
        assert_eq!(finish(&site, owner, video).await, Err(ApiError::Conflict));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn deleting_a_video_waits_for_the_piece_being_written() {
        let (site, dir) = site("delete");
        let site = Arc::new(site);
        let (owner, video) = creator(&site).await;
        let name = video.to_string();
        let (sender, frames) = tokio::sync::mpsc::channel::<Bytes>(1);
        let body = Body::from_stream(futures_util::stream::unfold(frames, |mut frames| async {
            let frame = frames.recv().await?;
            Some((Ok::<_, std::io::Error>(frame), frames))
        }));
        let piece = tokio::spawn({
            let (site, name) = (Arc::clone(&site), name.clone());
            async move { receive(&site, owner, &name, &at(0), body).await }
        });
        sender.send(Bytes::from_static(b"0123")).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let which = serde_json::json!({ "video": video });
        let deleting = tokio::spawn({
            let site = Arc::clone(&site);
            async move { crate::dispatch::run(&site, owner, "video.delete", which).await }
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!deleting.is_finished(), "the piece holds the upload");
        drop(sender);
        assert_eq!(piece.await.unwrap().status(), StatusCode::NO_CONTENT);
        assert_eq!(deleting.await.unwrap(), Ok(serde_json::json!({})));
        assert!(!site.config().uploads().join(&name).exists());
        let after = receive(&site, owner, &name, &at(4), bytes(b"x")).await;
        assert_eq!(after.status(), StatusCode::NOT_FOUND);
        assert!(
            !site.config().uploads().join(&name).exists(),
            "nothing makes it again"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn an_upload_too_large_is_refused_before_a_byte() {
        let (site, dir) = site("too-large");
        let (owner, _) = creator(&site).await;
        assert_eq!(
            start(&site, owner, 2 << 20).await,
            Err(ApiError::TooLarge { limit: 1 << 20 })
        );
        let viewer = Caller {
            role: youcraft_api::Role::Viewer,
            ..owner
        };
        assert_eq!(start(&site, viewer, 5).await, Err(ApiError::Forbidden));
        let _ = std::fs::remove_dir_all(dir);
    }
}
