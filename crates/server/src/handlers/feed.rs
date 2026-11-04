//! The feeds: candidates from the database, ordered by the recommendation algorithm with what
//! the viewer did, and the subscriptions feed, newest first.

use youcraft_api::commands::feed::{Home, Page, Subscriptions};
use youcraft_api::records::Feed;
use youcraft_api::{ApiError, VideoId};
use youcraft_recommend::default::Default as DefaultAlgorithm;
use youcraft_recommend::v1::{Algorithm, Candidate, Request, Surface, Viewer, Watched};
use youcraft_store::videos::Listed;

use crate::dispatch::Handle;
use crate::handlers::input::{next, window};
use crate::site::{Caller, Site, now};

/// How many recent videos the algorithm chooses from.
pub const CANDIDATES: u32 = 500;
/// How many videos one page of a feed holds.
pub const PAGE: u32 = 48;

impl Handle<Home> for Site {
    async fn handle(&self, caller: Caller, _: Page) -> Result<Feed, ApiError> {
        let listed = self.db(|store| store.recent_watchable(CANDIDATES)).await?;
        let viewer = self.viewer(caller).await?;
        Ok(ranked(Surface::Home, viewer, listed, PAGE))
    }
}

impl Handle<Subscriptions> for Site {
    async fn handle(&self, caller: Caller, page: Page) -> Result<Feed, ApiError> {
        let account = caller.signed_in()?;
        let window = window(page.after.as_deref(), PAGE)?;
        let found = self
            .db(move |store| store.subscription_feed(account, window))
            .await?;
        Ok(Feed {
            videos: found.items,
            next: next(window, found.more),
        })
    }
}

impl Site {
    /// What the recommendation algorithm may know of `caller`: what they watched, liked and
    /// subscribed to; nothing for someone signed out.
    pub async fn viewer(&self, caller: Caller) -> Result<Viewer, ApiError> {
        let Some(account) = caller.account else {
            return Ok(Viewer::default());
        };
        let signals = self.db(move |store| store.signals(account)).await?;
        Ok(Viewer {
            signed_in: true,
            watched: signals
                .watched
                .into_iter()
                .map(|(video, channel, at)| Watched {
                    video: video.to_string(),
                    channel: channel.to_string(),
                    at,
                })
                .collect(),
            liked: signals.liked.iter().map(VideoId::to_string).collect(),
            subscribed: signals.subscribed.iter().map(ToString::to_string).collect(),
        })
    }
}

/// The feed of `listed` as the default algorithm orders it for `surface` and `viewer`.
pub fn ranked(surface: Surface, viewer: Viewer, listed: Vec<Listed>, limit: u32) -> Feed {
    let request = Request {
        surface,
        now: now(),
        viewer,
        candidates: listed.iter().map(candidate).collect(),
        limit,
    };
    let ranking = DefaultAlgorithm.rank(&request).bounded_by(&request);
    let mut cards: Vec<_> = listed.into_iter().map(|l| Some(l.card)).collect();
    let videos = ranking
        .videos
        .iter()
        .filter_map(|id| id.parse::<VideoId>().ok())
        .filter_map(|id| {
            cards
                .iter_mut()
                .find(|c| c.as_ref().is_some_and(|c| c.id == id))
                .and_then(Option::take)
        })
        .collect();
    Feed { videos, next: None }
}

fn candidate(listed: &Listed) -> Candidate {
    let card = &listed.card;
    Candidate {
        video: card.id.to_string(),
        channel: card.channel.id.to_string(),
        title: card.title.clone(),
        published_at: card.published_at,
        duration_ms: card
            .duration
            .in_timescale(youcraft_api::time::MILLISECONDS)
            .map_or(0, |d| d.ticks),
        views: card.views,
        likes: listed.likes,
        dislikes: listed.dislikes,
        comments: listed.comments,
    }
}
