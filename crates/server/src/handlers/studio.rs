//! The studio's list of a channel's videos, which any of its members reads, and the rules of a
//! video's title and description.
//!
//! Each command on a channel's videos checks, besides the role it requires on the site, the role
//! the caller holds in the video's channel ([`Site::allow_on_video`]; an administrator may act on
//! any); a video of a channel they are no member of is answered as not found, so that a private
//! video's existence never leaks.

use youcraft_api::commands::channel::Shelf;
use youcraft_api::commands::studio::{Video, Videos};
use youcraft_api::commands::video::{DESCRIPTION_LIMIT, TITLE_LIMIT, Which};
use youcraft_api::records::{StudioPage, StudioVideo};
use youcraft_api::{ApiError, ChannelRole, Rule, VideoId};

use crate::dispatch::Handle;
use crate::handlers::input::{line, optional_text};
use crate::site::{Caller, Site};

/// Videos per page of the studio.
const STUDIO_PAGE: u32 = 30;

/// `title` as a video's title is kept, if it is one a video may have.
pub fn title(title: &str) -> Result<String, ApiError> {
    line("title", title, TITLE_LIMIT)
}

/// `description` as a video's description is kept, if it is one a video may have.
pub fn description(description: &str) -> Result<String, ApiError> {
    optional_text("description", description, DESCRIPTION_LIMIT)
}

pub async fn studio_video(site: &Site, video: VideoId) -> Result<StudioVideo, ApiError> {
    site.db(move |store| store.studio_video(video))
        .await?
        .ok_or(ApiError::NotFound)
}

impl Handle<Video> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<StudioVideo, ApiError> {
        self.allow_on_video(caller, which.video, ChannelRole::Viewer)
            .await?;
        studio_video(self, which.video).await
    }
}

impl Handle<Videos> for Site {
    async fn handle(&self, caller: Caller, shelf: Shelf) -> Result<StudioPage, ApiError> {
        let channel = shelf.channel;
        self.allow_in_channel(caller, channel, ChannelRole::Viewer)
            .await?;
        let after = match shelf.after {
            Some(after) => Some(
                after
                    .parse::<VideoId>()
                    .map_err(|_| ApiError::invalid("after", Rule::Malformed))?,
            ),
            None => None,
        };
        let mut videos = self
            .db(move |store| store.channel_videos(channel, after, STUDIO_PAGE + 1))
            .await?;
        let next = if videos.len() > STUDIO_PAGE as usize {
            videos.truncate(STUDIO_PAGE as usize);
            videos.last().map(|v| v.card.id.to_string())
        } else {
            None
        };
        Ok(StudioPage { videos, next })
    }
}
