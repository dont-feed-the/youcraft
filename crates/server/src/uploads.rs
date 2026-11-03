//! The bytes of uploads: `PATCH /upload/<video>` appends a piece at its offset, `HEAD` answers
//! the offset to resume from, as the tus protocol does.
//!
//! A piece is accepted only at the exact offset received so far, so a client that lost an answer
//! asks again and resends; the file is cut back to that offset before each write, so bytes that
//! reached the disk but not the database are written again, never twice. Pieces of one upload
//! never interleave. Only the video's channel (or an administrator) may send them.

use axum::body::Bytes;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use youcraft_api::{ApiError, VideoId};

use crate::handlers::studio::may_manage;
use crate::handlers::upload::PIECE;
use crate::http::error_response;
use crate::site::{Caller, Site};

/// The header that carries an offset, both ways.
pub const OFFSET: &str = "upload-offset";
/// The header that carries the size of the whole file.
pub const LENGTH: &str = "upload-length";

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

/// The upload of `video` that `caller` may send to.
async fn upload(
    site: &Site,
    caller: Caller,
    video: &str,
) -> Result<(VideoId, youcraft_store::uploads::Receiving), ApiError> {
    let video: VideoId = video.parse().map_err(|_| ApiError::NotFound)?;
    may_manage(site, caller, video).await?;
    let receiving = site
        .db(move |store| store.receiving(video))
        .await?
        .ok_or(ApiError::Conflict)?;
    Ok((video, receiving))
}

/// `HEAD /upload/<video>`: how many bytes have arrived, of how many.
pub async fn offset(site: &Site, caller: Caller, video: &str) -> Response {
    match upload(site, caller, video).await {
        Ok((_, receiving)) => with_offset(StatusCode::OK, receiving.received, Some(receiving.size)),
        Err(error) => error_response(&error),
    }
}

/// `PATCH /upload/<video>`: append `body` at the offset `headers` name.
pub async fn receive(
    site: &Site,
    caller: Caller,
    video: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Response {
    let Some(offset) = headers
        .get(OFFSET)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
    else {
        return error_response(&ApiError::invalid(OFFSET, "a byte offset"));
    };
    let _writing = site.writing.lock().await;
    let (video, receiving) = match upload(site, caller, video).await {
        Ok(found) => found,
        Err(error) => return error_response(&error),
    };
    if offset != receiving.received {
        return with_offset(
            StatusCode::CONFLICT,
            receiving.received,
            Some(receiving.size),
        );
    }
    let length = body.len() as u64;
    if length > PIECE {
        return error_response(&ApiError::TooLarge { limit: PIECE });
    }
    let received = offset.saturating_add(length);
    if received > receiving.size {
        return error_response(&ApiError::TooLarge {
            limit: receiving.size,
        });
    }
    let path = site.config().uploads().join(video.to_string());
    let written = async {
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .await?;
        file.set_len(offset).await?;
        file.seek(std::io::SeekFrom::End(0)).await?;
        file.write_all(&body).await?;
        file.sync_data().await
    };
    if let Err(e) = written.await {
        tracing::error!("upload {video}: {e}");
        return error_response(&ApiError::Internal);
    }
    match site
        .db(move |store| store.set_received(video, received))
        .await
    {
        Ok(()) => with_offset(StatusCode::NO_CONTENT, received, None),
        Err(error) => error_response(&error),
    }
}

#[cfg(test)]
pub mod tests {
    use std::path::PathBuf;

    use axum::http::HeaderMap;
    use youcraft_api::{AccountId, ChannelId, Role};
    use youcraft_store::accounts::NewAccount;

    use super::*;
    use crate::config::{Config, TranscodeLimits};
    use crate::site::Credential;

    /// A site in a fresh data directory, which the caller removes.
    pub fn site(name: &str) -> (Site, PathBuf) {
        let data = std::env::temp_dir().join(format!("youcraft-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let config = Config {
            data: data.clone(),
            site: data.join("site"),
            address: "127.0.0.1:0".parse().unwrap(),
            origin: "https://youcraft.test".to_owned(),
            google: None,
            upload_bytes: 1 << 20,
            transcode: TranscodeLimits {
                memory_bytes: 1 << 30,
                wall: std::time::Duration::from_secs(60),
            },
        };
        (Site::open(&config).unwrap(), data)
    }

    /// A creator, and a private video they started uploading: 10 bytes.
    pub async fn creator(site: &Site) -> (Caller, VideoId) {
        let account = AccountId::from_bits(1);
        site.store()
            .create_account(&NewAccount {
                id: account,
                channel: ChannelId::from_bits(2),
                google_subject: "g1",
                email: "ada@example.com",
                name: "Ada",
                handle: "ada",
                role: Role::Creator,
                now: 0,
            })
            .unwrap();
        let caller = Caller {
            account: Some(account),
            role: Role::Creator,
            credential: Credential::Token,
            viewer: crate::site::ViewerKey::NOBODY,
        };
        let input = serde_json::json!({ "title": "Waves", "size": 10 });
        let upload = crate::dispatch::run(site, caller, "upload.create", input)
            .await
            .unwrap();
        (caller, upload["video"].as_str().unwrap().parse().unwrap())
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

    #[tokio::test]
    async fn pieces_arrive_at_their_offset_once_and_become_the_original() {
        let (site, dir) = site("uploads");
        let (owner, video) = creator(&site).await;
        let name = video.to_string();
        let first = receive(&site, owner, &name, &at(0), Bytes::from_static(b"0123")).await;
        assert_eq!(
            (first.status(), offset_of(&first)),
            (StatusCode::NO_CONTENT, 4)
        );
        let again = receive(&site, owner, &name, &at(0), Bytes::from_static(b"0123")).await;
        assert_eq!(
            (again.status(), offset_of(&again)),
            (StatusCode::CONFLICT, 4),
            "a resent piece is refused"
        );
        let stranger = Caller {
            account: Some(youcraft_api::AccountId::from_bits(9)),
            ..owner
        };
        let theirs = receive(&site, stranger, &name, &at(4), Bytes::from_static(b"x")).await;
        assert_eq!(theirs.status(), StatusCode::NOT_FOUND);
        let beyond = receive(&site, owner, &name, &at(4), Bytes::from_static(b"4567890")).await;
        assert_eq!(beyond.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let early = crate::dispatch::run(
            &site,
            owner,
            "upload.finish",
            serde_json::json!({ "video": name }),
        )
        .await;
        assert_eq!(early, Err(ApiError::Conflict), "bytes are missing");
        let rest = receive(&site, owner, &name, &at(4), Bytes::from_static(b"456789")).await;
        assert_eq!(offset_of(&rest), 10);
        let asked = offset(&site, owner, &name).await;
        assert_eq!((asked.status(), offset_of(&asked)), (StatusCode::OK, 10));
        crate::dispatch::run(
            &site,
            owner,
            "upload.finish",
            serde_json::json!({ "video": name }),
        )
        .await
        .unwrap();
        let original = std::fs::read(site.config().originals().join(&name)).unwrap();
        assert_eq!(original, b"0123456789");
        let job = site.store().claim_job(0).unwrap().unwrap();
        assert_eq!(job.video, video);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn an_upload_too_large_is_refused_before_a_byte() {
        let (site, dir) = site("too-large");
        let (owner, _) = creator(&site).await;
        let input = serde_json::json!({ "title": "Huge", "size": 2 << 20 });
        let refused = crate::dispatch::run(&site, owner, "upload.create", input).await;
        assert_eq!(refused, Err(ApiError::TooLarge { limit: 1 << 20 }));
        let viewer = Caller {
            role: youcraft_api::Role::Viewer,
            ..owner
        };
        let input = serde_json::json!({ "title": "Mine", "size": 5 });
        assert_eq!(
            crate::dispatch::run(&site, viewer, "upload.create", input).await,
            Err(ApiError::Forbidden)
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
