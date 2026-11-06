//! Commands about one video: what its page shows, what to watch next, rating it, viewing it,
//! and its channel's editors changing it, choosing its thumbnail or deleting it.

use youcraft_api::commands::site::Nothing;
use youcraft_api::commands::video::{
    Delete, Edit, Get, NewThumbnail, Next, Rate, Rated, RemoveThumbnail, SetRating, SetThumbnail,
    Update, View, Which,
};
use youcraft_api::records::{Feed, Processing, Rating, StudioVideo, VideoPage};
use youcraft_api::{ApiError, ChannelRole, VideoId};
use youcraft_recommend::v1::Surface;
use youcraft_store::studio::VideoEdit;

use crate::dispatch::Handle;
use crate::handlers::feed::{CANDIDATES, PAGE, ranked};
use crate::handlers::studio::{description, studio_video, title};
use crate::picture::{self, Making};
use crate::site::{Caller, Site, now};

/// Seconds in a day, the span in which a viewer's views of a video count once.
const DAY: i64 = 86_400;

impl Site {
    /// The page of `video`, if `caller` may see it.
    pub async fn video_page(&self, caller: Caller, video: VideoId) -> Result<VideoPage, ApiError> {
        let reader = caller.reader();
        self.db(move |store| store.video_page(video, reader))
            .await?
            .ok_or(ApiError::NotFound)
    }

    /// Give `video` the chosen thumbnail `version`, or none, and forget the files of the one it
    /// had.
    pub async fn replace_thumbnail(
        &self,
        video: VideoId,
        version: Option<String>,
    ) -> Result<StudioVideo, ApiError> {
        let chosen = version.clone();
        let old = self
            .db(move |store| store.set_thumbnail(video, chosen.as_deref()))
            .await?;
        if old != version {
            picture::remove_thumbnail(self, video, old).await;
        }
        studio_video(self, video).await
    }

    /// Whether `video` is one whose media a chosen thumbnail may join: publishing a video replaces
    /// all of its media, so only what a ready one holds stays.
    async fn takes_thumbnail(&self, video: VideoId) -> Result<(), ApiError> {
        match studio_video(self, video).await?.processing {
            Processing::Ready => Ok(()),
            _ => Err(ApiError::Conflict),
        }
    }
}

impl Handle<Get> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<VideoPage, ApiError> {
        self.video_page(caller, which.video).await
    }
}

impl Handle<Next> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Feed, ApiError> {
        let page = self.video_page(caller, which.video).await?;
        let listed = self.db(|store| store.recent_watchable(CANDIDATES)).await?;
        let surface = Surface::UpNext {
            watching: which.video.to_string(),
            channel: page.card.channel.id.to_string(),
        };
        let viewer = self.viewer(caller).await?;
        Ok(ranked(surface, viewer, listed, PAGE))
    }
}

impl Handle<SetRating> for Site {
    async fn handle(&self, caller: Caller, rate: Rate) -> Result<Rated, ApiError> {
        caller.acting()?;
        if rate.rating != Rating::None {
            caller.may_act()?;
        }
        let (reader, now) = (caller.reader(), now());
        let likes = self
            .db(move |store| store.rate(reader, rate.video, rate.rating, now))
            .await?
            .ok_or(ApiError::NotFound)?;
        Ok(Rated {
            likes,
            rating: rate.rating,
        })
    }
}

impl Handle<View> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Nothing, ApiError> {
        let page = self.video_page(caller, which.video).await?;
        if !page.playable {
            return Ok(Nothing {});
        }
        let (video, viewer, now) = (which.video, caller.viewer, now());
        self.db(move |store| store.count_view(video, &viewer.0, now.div_euclid(DAY)))
            .await?;
        if let Some(channel) = caller.channel {
            self.db(move |store| store.remember_watched(channel, video, now))
                .await?;
        }
        Ok(Nothing {})
    }
}

impl Handle<Update> for Site {
    async fn handle(&self, caller: Caller, edit: Edit) -> Result<StudioVideo, ApiError> {
        self.allow_on_video(caller, edit.video, ChannelRole::Editor)
            .await?;
        let new_title = edit.title.as_deref().map(title).transpose()?;
        let new_description = edit.description.as_deref().map(description).transpose()?;
        let video = edit.video;
        self.db(move |store| {
            let change = VideoEdit {
                title: new_title.as_deref(),
                description: new_description.as_deref(),
                visibility: edit.visibility,
                comments_enabled: edit.comments_enabled,
            };
            store.edit_video(video, &change, now())
        })
        .await?;
        studio_video(self, video).await
    }
}

impl Handle<SetThumbnail> for Site {
    async fn handle(&self, caller: Caller, new: NewThumbnail) -> Result<StudioVideo, ApiError> {
        let video = new.video;
        self.allow_on_video(caller, video, ChannelRole::Editor)
            .await?;
        self.takes_thumbnail(video).await?;
        let made = picture::make(self, Making::Thumbnail, &new.image).await?;
        // Holds off its deletion, which would otherwise leave the new files behind, and asks
        // again, since it may have come while the picture was made.
        let _held = self.lock_upload(video).await?;
        if let Err(error) = self.takes_thumbnail(video).await {
            made.discard().await;
            return Err(error);
        }
        let version = made.version().to_owned();
        made.place(&picture::thumbnail_directory(self, video, &version))
            .await?;
        self.replace_thumbnail(video, Some(version)).await
    }
}

impl Handle<RemoveThumbnail> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<StudioVideo, ApiError> {
        self.allow_on_video(caller, which.video, ChannelRole::Editor)
            .await?;
        self.replace_thumbnail(which.video, None).await
    }
}

impl Handle<Delete> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Nothing, ApiError> {
        self.allow_on_video(caller, which.video, ChannelRole::Editor)
            .await?;
        let video = which.video;
        // Waits for a piece of its upload being written, which would otherwise make its file
        // again once deleted.
        let _held = self.lock_upload(video).await?;
        self.db(move |store| store.delete_video(video)).await?;
        crate::storage::remove_files_of(self.config(), video).await;
        Ok(Nothing {})
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::Request;
    use axum::http::{StatusCode, header};
    use serde_json::{Value, json};
    use youcraft_api::commands::video::THUMBNAIL_BYTES;
    use youcraft_api::records::Visibility;
    use youcraft_api::routes::media_files;
    use youcraft_api::time::MediaDuration;
    use youcraft_store::videos::NewVideo;

    use super::*;
    use crate::dispatch::run;
    use crate::uploads::tests::{creator, site};

    /// A public video of `caller`'s channel that is ready to play.
    fn ready(site: &Site, caller: Caller) -> VideoId {
        let video = VideoId::from_bits(99);
        site.store()
            .insert_ready_video(&NewVideo {
                id: video,
                channel: caller.channel.unwrap(),
                title: "Standing waves",
                description: "",
                visibility: Visibility::Public,
                duration: MediaDuration::from_millis(1000),
                width: 640,
                height: 360,
                published_at: 1,
            })
            .unwrap();
        video
    }

    #[tokio::test]
    async fn a_chosen_thumbnail_takes_the_place_of_the_last_and_of_its_files() {
        let (site, _dir) = site("chosen-thumbnail");
        let (owner, _) = creator(&site).await;
        let video = ready(&site, owner);
        let (first, second) = ("0000000000000001", "0000000000000002");
        let files = |version: &str| {
            let directory = picture::thumbnail_directory(&site, video, version);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join("720.jpg"), "jpeg").unwrap();
            directory
        };
        let (first_files, second_files) = (files(first), files(second));
        let shown = site
            .replace_thumbnail(video, Some(first.to_owned()))
            .await
            .unwrap();
        assert_eq!(shown.card.thumbnail.as_deref(), Some(first));
        site.replace_thumbnail(video, Some(second.to_owned()))
            .await
            .unwrap();
        assert!(!first_files.exists(), "the last one's files are gone");

        let file = format!("{}/720.jpg", media_files::chosen_thumbnail(second));
        let request = Request::builder().uri("/").body(Body::empty()).unwrap();
        let name = video.to_string();
        let served = crate::media::serve(&site, Caller::ANONYMOUS, &name, &file, request).await;
        assert_eq!(served.status(), StatusCode::OK);
        assert!(
            served.headers()[header::CACHE_CONTROL]
                .to_str()
                .unwrap()
                .contains("immutable"),
            "its address names its version, so it never changes"
        );

        let which = json!({ "video": name });
        let removed = run(&site, owner, "video.remove_thumbnail", which)
            .await
            .unwrap();
        assert_eq!(removed["card"]["thumbnail"], Value::Null);
        assert!(!second_files.exists());
    }

    #[tokio::test]
    async fn only_a_ready_video_takes_a_thumbnail_and_only_a_picture_that_fits() {
        let (site, _dir) = site("refused-thumbnail");
        let (owner, uploading) = creator(&site).await;
        let video = ready(&site, owner);
        let set = |video: VideoId, image: String| {
            let input = json!({ "video": video.to_string(), "image": image });
            run(&site, owner, "video.set_thumbnail", input)
        };
        assert!(matches!(
            set(video, "not base64!".to_owned()).await,
            Err(ApiError::Invalid { .. })
        ));
        assert!(matches!(
            set(video, "A".repeat(THUMBNAIL_BYTES / 3 * 4 + 8)).await,
            Err(ApiError::TooLarge { .. })
        ));
        assert!(matches!(
            set(uploading, "aGk=".to_owned()).await,
            Err(ApiError::Conflict)
        ));
    }
}
