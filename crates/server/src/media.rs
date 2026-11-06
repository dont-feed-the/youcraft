//! The media of videos, `GET /media/<video>/<file>`, served to whoever may watch the video, and
//! channels' photos, `GET /photos/<channel>/<version>/<size>.jpg`, served to anyone.
//!
//! The files of a video everyone may see (ready, public or unlisted) are anyone's and never
//! change, so caches keep them for a year; those of a video only its managers see (private, or
//! not ready yet) go to its channel and administrators only, and no shared cache keeps them.
//! Files are served with `Range` support, as players fetch segments by byte range.
//! A photo's address names its version, so its bytes never change either.

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use tower::ServiceExt;
use tower_http::services::ServeDir;
use youcraft_api::routes::{PHOTO_SIZES, photo_file};
use youcraft_api::{ApiError, ChannelId, VideoId};
use youcraft_store::videos::Audience;

use crate::http::error_response;
use crate::site::{Caller, Site};

/// Serve `file` of `video`'s media to `caller`, as `request` asks.
pub async fn serve(
    site: &Site,
    caller: Caller,
    video: &str,
    file: &str,
    mut request: Request,
) -> Response {
    let Ok(video) = video.parse::<VideoId>() else {
        return error_response(&ApiError::NotFound);
    };
    let reader = caller.reader();
    let cache = match site.db(move |store| store.audience(video, reader)).await {
        Ok(Some(Audience::Everyone)) => "public, max-age=31536000, immutable",
        Ok(Some(Audience::Managers)) => "private, max-age=3600",
        Ok(None) => return error_response(&ApiError::NotFound),
        Err(error) => return error_response(&error),
    };
    let Ok(uri) = format!("/{file}").parse() else {
        return error_response(&ApiError::NotFound);
    };
    *request.uri_mut() = uri;
    let directory = ServeDir::new(site.config().media().join(video.to_string()));
    match directory.oneshot(request).await {
        Ok(response) => {
            let mut response = response.map(Body::new).into_response();
            if response.status().is_success() {
                let headers = response.headers_mut();
                headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
                if file.ends_with(".m3u8") {
                    // RFC 8216's type, which every HLS player knows
                    headers.insert(
                        header::CONTENT_TYPE,
                        HeaderValue::from_static("application/vnd.apple.mpegurl"),
                    );
                }
            }
            response
        }
        Err(error) => match error {},
    }
}

/// Serve the photo `file` of `channel` at `version`, as `request` asks.
pub async fn serve_photo(
    site: &Site,
    (channel, version, file): (&str, &str, &str),
    mut request: Request,
) -> Response {
    let known = PHOTO_SIZES.iter().any(|&size| photo_file(size) == file);
    let Ok(channel) = channel.parse::<ChannelId>() else {
        return error_response(&ApiError::NotFound);
    };
    if !known || !crate::picture::is_version(version) {
        return error_response(&ApiError::NotFound);
    }
    let Ok(uri) = format!("/{file}").parse() else {
        return error_response(&ApiError::NotFound);
    };
    *request.uri_mut() = uri;
    let directory = ServeDir::new(crate::picture::photo_directory(site, channel, version));
    match directory.oneshot(request).await {
        Ok(response) => {
            let mut response = response.map(Body::new).into_response();
            if response.status().is_success() {
                response.headers_mut().insert(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("public, max-age=31536000, immutable"),
                );
            }
            response
        }
        Err(error) => match error {},
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::StatusCode;

    use super::*;
    use crate::uploads::tests::{creator, site};

    #[tokio::test]
    async fn a_private_video_plays_for_its_channel_only() {
        let (site, _dir) = site("media");
        let (owner, video) = creator(&site).await;
        let media = site.config().media().join(video.to_string());
        std::fs::create_dir_all(&media).unwrap();
        std::fs::write(media.join("master.m3u8"), "#EXTM3U\n").unwrap();
        let get = || Request::builder().uri("/").body(Body::empty()).unwrap();
        let name = video.to_string();
        let anonymous = serve(&site, Caller::ANONYMOUS, &name, "master.m3u8", get()).await;
        assert_eq!(anonymous.status(), StatusCode::NOT_FOUND);
        let theirs = serve(&site, owner, &name, "master.m3u8", get()).await;
        assert_eq!(theirs.status(), StatusCode::OK);
        assert_eq!(
            theirs.headers()[header::CACHE_CONTROL],
            "private, max-age=3600"
        );
        assert_eq!(
            theirs.headers()[header::CONTENT_TYPE],
            "application/vnd.apple.mpegurl"
        );
        let escape = serve(&site, owner, &name, "../../youcraft.sqlite", get()).await;
        assert_eq!(escape.status(), StatusCode::NOT_FOUND);
    }
}
