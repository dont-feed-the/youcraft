//! Version 1 of the recommendation contract.
//!
//! The site chooses the candidates and hands the algorithm a [`Request`]: the surface to fill,
//! the time, the viewer's own signals and each candidate's public features. The algorithm
//! answers a [`Ranking`]: candidate ids in the order to show them, at most `limit`; the site
//! drops any id that was not a candidate. Both travel as JSON with these field names, so an
//! algorithm can be written in any language. Identifiers are the 11-symbol spellings of
//! addresses; times are seconds since 1970 (UTC); durations are milliseconds.

use serde::{Deserialize, Serialize};

/// Where the ranking will be shown.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "surface", rename_all = "snake_case")]
pub enum Surface {
    /// The home page's grid.
    Home,
    /// The list beside the video being watched.
    UpNext {
        /// The video being watched.
        watching: String,
        /// Its channel.
        channel: String,
    },
}

/// One video the viewer watched.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watched {
    /// The video.
    pub video: String,
    /// Its channel.
    pub channel: String,
    /// When it was last watched.
    pub at: i64,
}

/// What the viewer has done, and only the viewer; empty for someone signed out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewer {
    /// Whether the viewer is signed in.
    pub signed_in: bool,
    /// The videos the viewer watched, most recent first.
    pub watched: Vec<Watched>,
    /// The videos the viewer liked.
    pub liked: Vec<String>,
    /// The channels the viewer subscribes to.
    pub subscribed: Vec<String>,
}

/// A video the algorithm may place, with its public features.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    /// The video.
    pub video: String,
    /// Its channel.
    pub channel: String,
    /// Its title.
    pub title: String,
    /// When it was published.
    pub published_at: i64,
    /// How long it lasts.
    pub duration_ms: u64,
    /// How many times it was watched.
    pub views: u64,
    /// How many people liked it.
    pub likes: u64,
    /// How many people disliked it.
    pub dislikes: u64,
    /// How many comments it has.
    pub comments: u64,
}

/// Everything an algorithm receives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// Where the ranking will be shown.
    #[serde(flatten)]
    pub surface: Surface,
    /// The time of the request.
    pub now: i64,
    /// The viewer's own signals.
    pub viewer: Viewer,
    /// The videos to choose from.
    pub candidates: Vec<Candidate>,
    /// The most videos to return.
    pub limit: u32,
}

/// What an algorithm returns.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ranking {
    /// Candidate ids, in the order to show them.
    pub videos: Vec<String>,
}

/// An implementation of this version of the contract.
pub trait Algorithm {
    /// Order the request's candidates.
    fn rank(&self, request: &Request) -> Ranking;
}

impl Ranking {
    /// This ranking kept to what `request` allows: candidates only, each once, at most `limit`.
    pub fn bounded_by(self, request: &Request) -> Ranking {
        let mut seen = std::collections::HashSet::new();
        let limit = usize::try_from(request.limit).unwrap_or(usize::MAX);
        let videos = self
            .videos
            .into_iter()
            .filter(|video| request.candidates.iter().any(|c| &c.video == video))
            .filter(|video| seen.insert(video.clone()))
            .take(limit)
            .collect();
        Ranking { videos }
    }
}
