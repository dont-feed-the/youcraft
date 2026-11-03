//! The records pages show: cards of videos and channels, the watch page, feeds.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::{ChannelId, VideoId};
use crate::time::MediaDuration;

/// A channel as a video card or a comment names it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelCard {
    /// Its identifier.
    pub id: ChannelId,
    /// Its handle, without the `@`.
    pub handle: String,
    /// Its name.
    pub name: String,
}

/// A video in a grid or a list: enough to draw its card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VideoCard {
    /// Its identifier.
    pub id: VideoId,
    /// Its title.
    pub title: String,
    /// The channel that published it.
    pub channel: ChannelCard,
    /// How long it lasts.
    pub duration: MediaDuration,
    /// How many times it was watched.
    pub views: u64,
    /// When it was published, in seconds since 1970 (UTC).
    pub published_at: i64,
}

/// A page of a feed, and where the next page starts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Feed {
    /// The videos of this page, in the order to show them.
    pub videos: Vec<VideoCard>,
    /// What to send for the next page; `None` at the end.
    pub next: Option<String>,
}

/// Everything the watch page shows about one video.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VideoPage {
    /// The video as its card shows it.
    pub card: VideoCard,
    /// Its description.
    pub description: String,
    /// How many people liked it.
    pub likes: u64,
    /// How many comments it has.
    pub comments: u64,
    /// Whether people may comment on it.
    pub comments_enabled: bool,
    /// Whether its renditions are ready to play.
    pub playable: bool,
}
