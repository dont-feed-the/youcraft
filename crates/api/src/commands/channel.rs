//! Commands about channels: their page, their videos and playlists, subscribing to them, and a
//! creator keeping an account from commenting on theirs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::playlist::PlaylistShelf;
use crate::commands::site::Nothing;
use crate::ids::ChannelId;
use crate::records::{ChannelCard, Feed};
use crate::role::Role;

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
    /// Whether it is the caller's own channel.
    pub owned: bool,
}

/// A channel, by its handle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Handle {
    /// Its handle, with or without `@`.
    pub handle: String,
}

/// A page of what a channel shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Shelf {
    /// The channel.
    pub channel: ChannelId,
    /// The `next` of the previous page; `None` for the first.
    #[serde(default)]
    pub after: Option<String>,
}

/// Whether the caller subscribes to a channel from now on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

/// Whether an account may comment on the caller's videos.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Barring {
    /// The handle of the account's channel, with or without `@`.
    pub handle: String,
    /// `true` to keep it from commenting, `false` to let it again.
    pub blocked: bool,
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
}

/// The videos a channel shows everyone, newest first.
pub struct Videos;

impl Command for Videos {
    const ID: &'static str = "channel.videos";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str = "List the public videos of a channel, newest first.";
    type Input = Shelf;
    type Output = Feed;
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
}

/// Subscribe to a channel, or unsubscribe.
pub struct Subscribe;

impl Command for Subscribe {
    const ID: &'static str = "channel.subscribe";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str =
        "Subscribe to a channel or unsubscribe from it; answers its subscribers.";
    type Input = Subscribing;
    type Output = Subscription;
}

/// The channels the caller subscribes to.
pub struct Subscriptions;

impl Command for Subscriptions {
    const ID: &'static str = "channel.subscriptions";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str = "List the channels you subscribe to, by name.";
    type Input = Nothing;
    type Output = Vec<ChannelCard>;
}

/// Keep an account from commenting on the caller's videos, or let it again.
pub struct Block;

impl Command for Block {
    const ID: &'static str = "channel.block";
    const REQUIRES: Role = Role::Creator;
    const SUMMARY: &'static str = "Keep an account from commenting on your channel's videos, or let it again, by its channel's handle.";
    type Input = Barring;
    type Output = Nothing;
}
