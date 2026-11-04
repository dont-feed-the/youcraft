//! The studio's list of a channel's videos, and what every command on a channel's own videos
//! checks.
//!
//! Each such command checks, besides the role it requires, that the video is the caller's own
//! (an administrator may act on any); a video that is someone else's is answered as not found,
//! so that a private video's existence never leaks.

use youcraft_api::commands::feed::Page;
use youcraft_api::commands::studio::{Video, Videos};
use youcraft_api::commands::video::{DESCRIPTION_LIMIT, TITLE_LIMIT, Which};
use youcraft_api::records::{StudioPage, StudioVideo};
use youcraft_api::{ApiError, Role, Rule, VideoId};

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

/// Allow `caller` to manage `video`, which must be theirs unless they run the site.
pub async fn may_manage(site: &Site, caller: Caller, video: VideoId) -> Result<(), ApiError> {
    let owner = site.db(move |store| store.owner_of(video)).await?;
    match owner {
        Some(owner) if caller.account == Some(owner) || caller.role.allows(Role::Admin) => Ok(()),
        _ => Err(ApiError::NotFound),
    }
}

pub async fn studio_video(site: &Site, video: VideoId) -> Result<StudioVideo, ApiError> {
    site.db(move |store| store.studio_video(video))
        .await?
        .ok_or(ApiError::NotFound)
}

impl Handle<Video> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<StudioVideo, ApiError> {
        may_manage(self, caller, which.video).await?;
        studio_video(self, which.video).await
    }
}

impl Handle<Videos> for Site {
    async fn handle(&self, caller: Caller, page: Page) -> Result<StudioPage, ApiError> {
        let account = caller.account.ok_or(ApiError::SignInRequired)?;
        let after = match page.after {
            Some(after) => Some(
                after
                    .parse::<VideoId>()
                    .map_err(|_| ApiError::invalid("after", Rule::Malformed))?,
            ),
            None => None,
        };
        let channel = self
            .db(move |store| store.channel_of(account))
            .await?
            .ok_or(ApiError::Forbidden)?;
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
