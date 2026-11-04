//! Commands about one video.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::site::Nothing;
use crate::ids::VideoId;
use crate::records::{Feed, Rating, StudioVideo, VideoPage, Visibility};
use crate::role::Role;

/// The longest title of a video, in characters, as on YouTube.
pub const TITLE_LIMIT: usize = 100;
/// The longest description of a video, in characters, as on YouTube.
pub const DESCRIPTION_LIMIT: usize = 5_000;

/// Which video.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Which {
    /// The video.
    pub video: VideoId,
}

/// Everything the watch page shows about a video the caller may see.
pub struct Get;

impl Command for Get {
    const ID: &'static str = "video.get";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str =
        "Get what the watch page shows about a video: title, channel, counts, description.";
    type Input = Which;
    type Output = VideoPage;
}

/// What to watch after a video, as the caller's recommendation algorithm orders it.
pub struct Next;

impl Command for Next {
    const ID: &'static str = "video.next";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str = "List the videos to watch next beside a video, in the order the recommendation algorithm chose.";
    type Input = Which;
    type Output = Feed;
}

/// What to change about a video; what is `None` stays.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    /// The video.
    pub video: VideoId,
    /// Its new title: 1 to [`TITLE_LIMIT`] characters, on one line.
    #[serde(default)]
    #[schemars(length(min = 1, max = TITLE_LIMIT))]
    pub title: Option<String>,
    /// Its new description: at most [`DESCRIPTION_LIMIT`] characters.
    #[serde(default)]
    #[schemars(length(max = DESCRIPTION_LIMIT))]
    pub description: Option<String>,
    /// Who may now see it.
    #[serde(default)]
    pub visibility: Option<Visibility>,
    /// Whether people may now comment on it.
    #[serde(default)]
    pub comments_enabled: Option<bool>,
}

/// Change a video of the caller's channel.
pub struct Update;

impl Command for Update {
    const ID: &'static str = "video.update";
    const REQUIRES: Role = Role::Creator;
    const SUMMARY: &'static str =
        "Change the title, description, visibility or comment setting of one of your videos.";
    type Input = Edit;
    type Output = StudioVideo;
}

/// Delete a video of the caller's channel, with its media; blocked or not, what one published
/// is one's own to take back.
pub struct Delete;

impl Command for Delete {
    const ID: &'static str = "video.delete";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str =
        "Delete one of your videos, with its comments and its media, for good.";
    type Input = Which;
    type Output = ();
}

/// A rating to give a video.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rate {
    /// The video.
    pub video: VideoId,
    /// The caller's rating from now on; [`Rating::None`] takes theirs back.
    pub rating: Rating,
}

/// A video's likes once the caller rated it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Rated {
    /// How many people like it now.
    pub likes: u64,
    /// The caller's rating.
    pub rating: Rating,
}

/// Like a video, dislike it, or take the rating back; a blocked account may only take it back.
pub struct SetRating;

impl Command for SetRating {
    const ID: &'static str = "video.rate";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str =
        "Like or dislike a video, or take your rating back; answers its likes.";
    type Input = Rate;
    type Output = Rated;
}

/// Count that the caller started watching a video.
pub struct View;

impl Command for View {
    const ID: &'static str = "video.view";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str = "Count a view of a video, once a day per viewer, and keep it in the signed-in viewer's history unless they paused it.";
    type Input = Which;
    type Output = Nothing;
}
