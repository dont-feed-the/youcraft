//! Commands about one video.

use youcraft_api::ApiError;
use youcraft_api::commands::video::{Get, Next, Which};
use youcraft_api::records::{Feed, VideoPage};
use youcraft_recommend::v1::{Surface, Viewer};

use crate::dispatch::Handle;
use crate::handlers::feed::{PAGE, ranked};
use crate::site::{Caller, Site};

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
