//! Commands about one video.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::ids::VideoId;
use crate::records::{Feed, VideoPage};
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
