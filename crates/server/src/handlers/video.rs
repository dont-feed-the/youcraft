//! Commands about one video: what its page shows, what to watch next, rating it, viewing it,
//! and its channel's editors changing or deleting it.

use youcraft_api::commands::site::Nothing;
use youcraft_api::commands::video::{
    Delete, Edit, Get, Next, Rate, Rated, SetRating, Update, View, Which,
};
use youcraft_api::records::{Feed, Rating, StudioVideo, VideoPage};
use youcraft_api::{ApiError, ChannelRole, VideoId};
use youcraft_recommend::v1::Surface;
use youcraft_store::studio::VideoEdit;

use crate::dispatch::Handle;
use crate::handlers::feed::{CANDIDATES, PAGE, ranked};
use crate::handlers::studio::{description, studio_video, title};
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
