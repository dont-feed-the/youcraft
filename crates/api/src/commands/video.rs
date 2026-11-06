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
/// The most bytes the file of a video's thumbnail has, as on YouTube.
pub const THUMBNAIL_BYTES: usize = 2 << 20;

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
    const WRITES: bool = false;
}

/// What to watch after a video, as the caller's recommendation algorithm orders it.
pub struct Next;

impl Command for Next {
    const ID: &'static str = "video.next";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str = "List the videos to watch next beside a video, in the order the recommendation algorithm chose.";
    type Input = Which;
    type Output = Feed;
    const WRITES: bool = false;
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

/// Change a video of a channel the caller edits.
pub struct Update;

impl Command for Update {
    const ID: &'static str = "video.update";
    const REQUIRES: Role = Role::Creator;
    const SUMMARY: &'static str = "Change the title, description, visibility or comment setting of a video of a channel you edit.";
    type Input = Edit;
    type Output = StudioVideo;
    const WRITES: bool = true;
}

/// A thumbnail to choose for a video.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewThumbnail {
    /// The video, which is ready to play.
    pub video: VideoId,
    /// The file, JPEG, PNG or WebP of at most [`THUMBNAIL_BYTES`] bytes, in base64 (RFC 4648, with
    /// or without padding). A picture that is not 16:9 gets black bars.
    #[schemars(length(min = 1, max = THUMBNAIL_BYTES.div_ceil(3) * 4))]
    pub image: String,
}

/// Choose the thumbnail of a video of a channel the caller edits, in place of the one it shows.
pub struct SetThumbnail;

impl Command for SetThumbnail {
    const ID: &'static str = "video.set_thumbnail";
    const REQUIRES: Role = Role::Creator;
    const SUMMARY: &'static str = "Choose the thumbnail of a ready video of a channel you edit: a JPEG, PNG or WebP file of at most 2 MB, in base64; a picture that is not 16:9 gets black bars.";
    type Input = NewThumbnail;
    type Output = StudioVideo;
    const WRITES: bool = true;
}

/// Take back the thumbnail chosen for a video of a channel the caller edits; blocked or not, what
/// one published is one's own to take back.
pub struct RemoveThumbnail;

impl Command for RemoveThumbnail {
    const ID: &'static str = "video.remove_thumbnail";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str =
        "Remove the thumbnail chosen for a video of a channel you edit; a frame of it shows again.";
    type Input = Which;
    type Output = StudioVideo;
    const WRITES: bool = true;
}

/// Delete a video of a channel the caller edits, with its media; blocked or not, what one
/// published is one's own to take back.
pub struct Delete;

impl Command for Delete {
    const ID: &'static str = "video.delete";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str =
        "Delete a video of a channel you edit, with its comments and its media, for good.";
    type Input = Which;
    type Output = Nothing;
    const WRITES: bool = true;
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

/// Like a video as the channel the caller acts as, dislike it, or take the rating back; a blocked
/// account may only take it back.
pub struct SetRating;

impl Command for SetRating {
    const ID: &'static str = "video.rate";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str =
        "Like or dislike a video, or take your rating back; answers its likes.";
    type Input = Rate;
    type Output = Rated;
    const WRITES: bool = true;
}

/// Count that the caller started watching a video.
pub struct View;

impl Command for View {
    const ID: &'static str = "video.view";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str = "Count a view of a video, once a day per viewer, and keep it in the history of the channel the viewer acts as unless it is paused.";
    type Input = Which;
    type Output = Nothing;
    const WRITES: bool = true;
}
