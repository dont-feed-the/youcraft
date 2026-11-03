//! Commands about one video.

use youcraft_api::ApiError;
use youcraft_api::commands::video::{Delete, Edit, Get, Next, Update, Which};
use youcraft_api::records::{Feed, StudioVideo, VideoPage};
use youcraft_recommend::v1::{Surface, Viewer};
use youcraft_store::studio::VideoEdit;

use crate::dispatch::Handle;
use crate::handlers::feed::{PAGE, ranked};
use crate::handlers::studio::{description, may_manage, studio_video, title};
use crate::site::{Caller, Site, now};

impl Handle<Get> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<VideoPage, ApiError> {
        let viewer = caller.account;
        self.db(move |store| store.video_page(which.video, viewer))
            .await?
            .ok_or(ApiError::NotFound)
    }
}

impl Handle<Next> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Feed, ApiError> {
        let viewer = caller.account;
        let page = self
            .db(move |store| store.video_page(which.video, viewer))
            .await?
            .ok_or(ApiError::NotFound)?;
        let listed = self.db(|store| store.recent_watchable(500)).await?;
        let surface = Surface::UpNext {
            watching: which.video.to_string(),
            channel: page.card.channel.id.to_string(),
        };
        Ok(ranked(surface, Viewer::default(), listed, PAGE))
    }
}

impl Handle<Update> for Site {
    async fn handle(&self, caller: Caller, edit: Edit) -> Result<StudioVideo, ApiError> {
        may_manage(self, caller, edit.video).await?;
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
    async fn handle(&self, caller: Caller, which: Which) -> Result<(), ApiError> {
        may_manage(self, caller, which.video).await?;
        let video = which.video;
        self.db(move |store| store.delete_video(video)).await?;
        let config = self.config();
        let name = video.to_string();
        for file in [config.uploads().join(&name), config.originals().join(&name)] {
            let _ = tokio::fs::remove_file(file).await;
        }
        for dir in [config.media().join(&name), config.work().join(&name)] {
            let _ = tokio::fs::remove_dir_all(dir).await;
        }
        Ok(())
    }
}
