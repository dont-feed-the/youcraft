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
    /// How many people subscribe to its channel.
    pub subscribers: u64,
    /// What the caller said of it.
    pub rating: Rating,
    /// Whether the caller subscribes to its channel.
    pub subscribed: bool,
    /// Whether it is the caller's own video.
    pub owned: bool,
}

/// What a person said of a video: nothing, that they like it, or that they do not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    /// Nothing.
    #[default]
    None,
    /// They like it.
    Like,
    /// They do not like it.
    Dislike,
}

impl Rating {
    /// The value the database keeps: +1, -1, or no row for [`Rating::None`].
    pub fn value(self) -> Option<i64> {
        match self {
            Rating::None => None,
            Rating::Like => Some(1),
            Rating::Dislike => Some(-1),
        }
    }

    /// The rating the database keeps as `value`.
    pub fn from_value(value: Option<i64>) -> Rating {
        match value {
            Some(1) => Rating::Like,
            Some(-1) => Rating::Dislike,
            _ => Rating::None,
        }
    }
}

/// The storyboard of a video: small tiles the seek bar previews, laid out row by row in sheets
/// of `columns × rows` tiles; tile `i` shows the time `i × interval`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Storyboard {
    /// The time between two tiles.
    pub interval: MediaDuration,
    /// A tile's size in pixels.
    pub tile_width: u32,
    /// A tile's size in pixels.
    pub tile_height: u32,
    /// Tiles per sheet row.
    pub columns: u32,
    /// Rows per full sheet; the last sheet may have fewer.
    pub rows: u32,
    /// How many sheets there are.
    pub sheets: u32,
}

/// Who may see a video or a playlist.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Everyone; it appears in feeds and search.
    Public,
    /// Anyone with its address; it appears nowhere.
    Unlisted,
    /// Its channel only.
    #[default]
    Private,
}

impl Visibility {
    /// Its name in the database and in the API.
    pub fn as_str(self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Unlisted => "unlisted",
            Visibility::Private => "private",
        }
    }

    /// The visibility named `name`.
    pub fn from_name(name: &str) -> Option<Visibility> {
        [
            Visibility::Public,
            Visibility::Unlisted,
            Visibility::Private,
        ]
        .into_iter()
        .find(|v| v.as_str() == name)
    }
}

/// Where a video is on its way from upload to playable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Processing {
    /// Its file is arriving: `received` bytes of `size`.
    Uploading {
        /// Bytes received.
        received: u64,
        /// Bytes in all.
        size: u64,
    },
    /// Its renditions are being made: `permille` thousandths done.
    Processing {
        /// Thousandths done.
        permille: u32,
    },
    /// It plays.
    Ready,
    /// It could not be made playable: `unsupported` (not a video the site reads), `decode` (the
    /// file is damaged) or `internal`.
    Failed {
        /// Why.
        reason: String,
    },
}

/// A video as its channel sees it in the studio.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StudioVideo {
    /// The video as its card shows it.
    pub card: VideoCard,
    /// Its description.
    pub description: String,
    /// Who may see it.
    pub visibility: Visibility,
    /// Whether it plays yet.
    pub processing: Processing,
    /// Whether people may comment on it.
    pub comments_enabled: bool,
    /// How many people liked it.
    pub likes: u64,
    /// How many comments it has.
    pub comments: u64,
}

/// A page of the studio's videos, newest first, and where the next page starts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StudioPage {
    /// The videos of this page.
    pub videos: Vec<StudioVideo>,
    /// What to send for the next page; `None` at the end.
    pub next: Option<String>,
}

/// An upload in progress: where to send its bytes, and how many have arrived.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Upload {
    /// The video it becomes.
    pub video: VideoId,
    /// Where its bytes go: `PATCH` with the header `Upload-Offset`, one piece at a time.
    pub address: String,
    /// The most bytes one `PATCH` may carry.
    pub piece: u64,
    /// Bytes received so far: the offset of the next piece.
    pub received: u64,
}
