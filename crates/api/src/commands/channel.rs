//! Commands about channels: making and deleting them, their page, their videos and playlists,
//! subscribing to them, and a channel keeping another from commenting on its videos.
//!
//! A channel is a public identity that one or more people run (see `member`). Whatever the caller
//! does toward a channel, subscribing or blocking, they do as the channel they act as (see
//! `account`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::playlist::PlaylistShelf;
use crate::commands::site::Nothing;
use crate::ids::ChannelId;
use crate::records::{ChannelCard, Feed};
use crate::role::Role;

/// The longest name of a channel, in characters: what its owner may give it, and what is kept
/// of the name Google gives a new account.
pub const NAME_LIMIT: usize = 50;

/// What the top of a channel's page shows about it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelPage {
    /// The channel.
    pub card: ChannelCard,
    /// What the channel says about itself.
    pub description: String,
    /// How many people subscribe to it.
    pub subscribers: u64,
    /// How many videos it shows everyone.
    pub videos: u64,
    /// Whether the caller subscribes to it.
    pub subscribed: bool,
    /// Whether it is the channel the caller acts as.
    pub owned: bool,
}

/// A channel, by its identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Which {
    /// The channel.
    pub channel: ChannelId,
}

/// A channel to make.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewChannel {
    /// Its name: 1 to [`NAME_LIMIT`] characters on one line, any script. Its handle comes from
    /// it, and never changes.
    #[schemars(length(min = 1, max = NAME_LIMIT))]
    pub name: String,
}

/// A channel, by its handle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Handle {
    /// Its handle, with or without `@`.
    pub handle: String,
}

/// A page of what a channel shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Shelf {
    /// The channel.
    pub channel: ChannelId,
    /// The `next` of the previous page; `None` for the first.
    #[serde(default)]
    pub after: Option<String>,
}

/// Whether the caller subscribes to a channel from now on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Subscribing {
    /// The channel.
    pub channel: ChannelId,
    /// `true` to subscribe, `false` to unsubscribe.
    pub subscribed: bool,
}

/// A channel's subscribers once the caller subscribed or unsubscribed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Subscription {
    /// How many people subscribe to it now.
    pub subscribers: u64,
    /// Whether the caller does.
    pub subscribed: bool,
}

/// Whether a channel may comment on the videos of the channel the caller acts as.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Barring {
    /// The handle of the channel, with or without `@`.
    pub handle: String,
    /// `true` to keep it from commenting, `false` to let it again.
    pub blocked: bool,
}

/// Make a channel, which the caller owns and then acts as.
pub struct Create;

impl Command for Create {
    const ID: &'static str = "channel.create";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str = "Make a channel that you own, named as you say, with a handle that comes from its name; you then act as it.";
    type Input = NewChannel;
    type Output = ChannelCard;
    const WRITES: bool = true;
}

/// Delete a channel, with its videos, comments and playlists, for good.
pub struct Delete;

impl Command for Delete {
    const ID: &'static str = "channel.delete";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str =
        "Delete a channel you own, with its videos, comments, playlists and members, for good.";
    type Input = Which;
    type Output = Nothing;
    const WRITES: bool = true;
}

/// The top of a channel's page.
pub struct Get;

impl Command for Get {
    const ID: &'static str = "channel.get";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str =
        "Get what a channel's page shows about it, by its handle: name, description, counts.";
    type Input = Handle;
    type Output = ChannelPage;
    const WRITES: bool = false;
}

/// The videos a channel shows everyone, newest first.
pub struct Videos;

impl Command for Videos {
    const ID: &'static str = "channel.videos";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str = "List the public videos of a channel, newest first.";
    type Input = Shelf;
    type Output = Feed;
    const WRITES: bool = false;
}

/// The playlists a channel shows everyone.
pub struct Playlists;

impl Command for Playlists {
    const ID: &'static str = "channel.playlists";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str =
        "List the public playlists of a channel, the most recently changed first.";
    type Input = Shelf;
    type Output = PlaylistShelf;
    const WRITES: bool = false;
}

/// Subscribe the channel the caller acts as to a channel, or unsubscribe it; a blocked account
/// may only unsubscribe.
pub struct Subscribe;

impl Command for Subscribe {
    const ID: &'static str = "channel.subscribe";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str =
        "Subscribe to a channel or unsubscribe from it; answers its subscribers.";
    type Input = Subscribing;
    type Output = Subscription;
    const WRITES: bool = true;
}

/// The channels that the channel the caller acts as subscribes to.
pub struct Subscriptions;

impl Command for Subscriptions {
    const ID: &'static str = "channel.subscriptions";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str = "List the channels you subscribe to, by name.";
    type Input = Nothing;
    type Output = Vec<ChannelCard>;
    const WRITES: bool = false;
}

/// Keep a channel from commenting on the videos of the channel the caller acts as, or let it
/// again.
pub struct Block;

impl Command for Block {
    const ID: &'static str = "channel.block";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str =
        "Keep a channel from commenting on your channel's videos, or let it again, by its handle.";
    type Input = Barring;
    type Output = Nothing;
    const WRITES: bool = true;
}
