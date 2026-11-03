//! Commands about one video.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::ids::VideoId;
use crate::records::{Feed, StudioVideo, VideoPage, Visibility};
use crate::role::Role;

/// Which video.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
pub struct Edit {
    /// The video.
    pub video: VideoId,
    /// Its new title.
    #[serde(default)]
    pub title: Option<String>,
    /// Its new description.
    #[serde(default)]
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

/// Delete a video of the caller's channel, with its media.
pub struct Delete;

impl Command for Delete {
    const ID: &'static str = "video.delete";
    const REQUIRES: Role = Role::Creator;
    const SUMMARY: &'static str =
        "Delete one of your videos, with its comments and its media, for good.";
    type Input = Which;
    type Output = ();
}
