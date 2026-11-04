//! Commands about lists of videos: playlists people make, their Watch later list, the videos
//! they liked.
//!
//! A playlist is its owner's to change; others see it if it is public or unlisted. Watch later
//! and Liked videos are private, and Liked videos changes only as its owner rates videos.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::site::Nothing;
use crate::ids::{List, PlaylistId, VideoId};
use crate::records::{ChannelCard, VideoCard, Visibility};
use crate::role::Role;

/// The longest title of a playlist, in characters.
pub const TITLE_LIMIT: usize = 150;
/// The longest description of a playlist, in characters.
pub const DESCRIPTION_LIMIT: usize = 5_000;
/// The most videos a list holds.
pub const VIDEO_LIMIT: u64 = 5_000;
/// The most playlists one account makes.
pub const PLAYLIST_LIMIT: u64 = 1_000;

/// A list as a grid or a menu shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlaylistCard {
    /// The list.
    pub list: List,
    /// Its title; empty for Watch later and Liked videos, which pages name themselves.
    pub title: String,
    /// Who may see it.
    pub visibility: Visibility,
    /// How many of the videos it holds its reader may see.
    pub videos: u64,
    /// Its owner's channel.
    pub owner: ChannelCard,
    /// When it last changed, in seconds since 1970 (UTC).
    pub updated_at: i64,
    /// The video whose thumbnail stands for it: the first its reader may see.
    pub first: Option<VideoId>,
    /// Whether it holds the video `playlist.list` was asked about; `false` otherwise.
    pub contains: bool,
}

/// A page of playlists, and where the next one starts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlaylistShelf {
    /// The playlists.
    pub playlists: Vec<PlaylistCard>,
    /// What to send for the next page; `None` at the end.
    pub next: Option<String>,
}

/// A list's page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlaylistPage {
    /// The list.
    pub card: PlaylistCard,
    /// What its owner says about it.
    pub description: String,
    /// Its videos the caller may see, in its order.
    pub videos: Vec<VideoCard>,
    /// What to send for the next page; `None` at the end.
    pub next: Option<String>,
    /// Whether the caller may add, remove and move its videos.
    pub editable: bool,
}

/// A new playlist.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewPlaylist {
    /// Its title: 1 to [`TITLE_LIMIT`] characters, on one line.
    #[schemars(length(min = 1, max = TITLE_LIMIT))]
    pub title: String,
    /// Who may see it.
    #[serde(default)]
    pub visibility: Visibility,
    /// A first video to put in it.
    #[serde(default)]
    pub video: Option<VideoId>,
}

/// What to change about a playlist; what is `None` stays.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    /// The playlist.
    pub playlist: PlaylistId,
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
}

/// Which playlist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Which {
    /// The playlist.
    pub playlist: PlaylistId,
}

/// A video and a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Item {
    /// The list: Watch later or a playlist of the caller's.
    pub list: List,
    /// The video.
    pub video: VideoId,
}

/// Where a video of a list goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Placing {
    /// The list: Watch later or a playlist of the caller's.
    pub list: List,
    /// The video.
    pub video: VideoId,
    /// Its place from now on, counted from 0; past the end means last.
    pub position: u64,
}

/// The caller's lists, and which of them hold a video.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Mine {
    /// The video to tell about, as the Save menu asks.
    #[serde(default)]
    pub containing: Option<VideoId>,
}

/// A page of a list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Open {
    /// The list.
    pub list: List,
    /// The `next` of the previous page; `None` for the first.
    #[serde(default)]
    pub after: Option<String>,
}

/// Make a playlist.
pub struct Create;

impl Command for Create {
    const ID: &'static str = "playlist.create";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str = "Make a playlist, empty or holding a first video.";
    type Input = NewPlaylist;
    type Output = PlaylistCard;
}

/// Change a playlist's title, description or visibility.
pub struct Update;

impl Command for Update {
    const ID: &'static str = "playlist.update";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str =
        "Change the title, description or visibility of one of your playlists.";
    type Input = Edit;
    type Output = PlaylistCard;
}

/// Delete a playlist; its videos stay.
pub struct Delete;

impl Command for Delete {
    const ID: &'static str = "playlist.delete";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str = "Delete one of your playlists; the videos in it stay.";
    type Input = Which;
    type Output = Nothing;
}

/// Put a video at the end of a list.
pub struct Add;

impl Command for Add {
    const ID: &'static str = "playlist.add";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str =
        "Put a video at the end of your Watch later list or of one of your playlists.";
    type Input = Item;
    type Output = PlaylistCard;
}

/// Take a video out of a list.
pub struct Remove;

impl Command for Remove {
    const ID: &'static str = "playlist.remove";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str =
        "Take a video out of your Watch later list or of one of your playlists.";
    type Input = Item;
    type Output = PlaylistCard;
}

/// Move a video within a list.
pub struct Move;

impl Command for Move {
    const ID: &'static str = "playlist.move";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str =
        "Move a video to another place in your Watch later list or one of your playlists.";
    type Input = Placing;
    type Output = Nothing;
}

/// The caller's lists.
pub struct Lists;

impl Command for Lists {
    const ID: &'static str = "playlist.list";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str = "List your Watch later list, your liked videos and your playlists, telling which hold a given video.";
    type Input = Mine;
    type Output = Vec<PlaylistCard>;
}

/// A page of a list the caller may see.
pub struct Get;

impl Command for Get {
    const ID: &'static str = "playlist.get";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str =
        "Get a list and a page of its videos: WL, LL, or PL and a playlist's identifier.";
    type Input = Open;
    type Output = PlaylistPage;
}
