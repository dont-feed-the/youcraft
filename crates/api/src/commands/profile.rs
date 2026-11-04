//! Commands that change how a channel shows itself: its name, its description and its photo. Its
//! handle never changes, so no link to it ever breaks.
//!
//! Its owners run them, viewers of the site too, since a channel's name and photo stand beside
//! its comments; administrators moderate others' in `admin`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::channel::{NAME_LIMIT, Which};
use crate::ids::ChannelId;
use crate::records::ChannelCard;
use crate::role::Role;

/// The most characters a channel's description has.
pub const DESCRIPTION_LIMIT: usize = 1000;
/// The most bytes the file of a channel's photo has.
pub const PHOTO_BYTES: usize = 5 << 20;

/// What changes about a channel; what is left out stays.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Customization {
    /// The channel, which the caller owns.
    pub channel: ChannelId,
    /// Its new name: 1 to [`NAME_LIMIT`] characters on one line, any script, which other
    /// channels may share.
    #[serde(default)]
    #[schemars(length(min = 1, max = NAME_LIMIT))]
    pub name: Option<String>,
    /// Its new description: at most [`DESCRIPTION_LIMIT`] characters.
    #[serde(default)]
    #[schemars(length(max = DESCRIPTION_LIMIT))]
    pub description: Option<String>,
}

/// How a channel shows itself, once changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Profile {
    /// The channel, with its name and photo.
    pub card: ChannelCard,
    /// What it says about itself.
    pub description: String,
}

/// A new photo for a channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewPhoto {
    /// The channel, which the caller owns.
    pub channel: ChannelId,
    /// The file, JPEG, PNG or WebP of at most [`PHOTO_BYTES`] bytes, in base64 (RFC 4648, with
    /// or without padding). Its centre is cut square.
    #[schemars(length(min = 1, max = PHOTO_BYTES.div_ceil(3) * 4))]
    pub image: String,
}

/// Change the name or the description of a channel the caller owns.
pub struct Update;

impl Command for Update {
    const ID: &'static str = "channel.update";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str =
        "Change the name or the description of a channel you own; its handle stays.";
    type Input = Customization;
    type Output = Profile;
    const WRITES: bool = true;
}

/// Give a channel the caller owns a photo, in place of the one it had.
pub struct SetPhoto;

impl Command for SetPhoto {
    const ID: &'static str = "channel.set_photo";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str = "Give a channel you own a photo: a JPEG, PNG or WebP file of at most 5 MB, in base64, cut square at its centre.";
    type Input = NewPhoto;
    type Output = Profile;
    const WRITES: bool = true;
}

/// Take the photo of a channel the caller owns away; its initial shows again.
pub struct RemovePhoto;

impl Command for RemovePhoto {
    const ID: &'static str = "channel.remove_photo";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str =
        "Remove the photo of a channel you own; its initial shows instead.";
    type Input = Which;
    type Output = Profile;
    const WRITES: bool = true;
}
