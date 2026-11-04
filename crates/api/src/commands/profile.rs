//! Commands that change how the caller's channel shows itself: its name, its description and
//! its photo. Its handle never changes, so no link to it ever breaks.
//!
//! Every account that is not blocked runs them on its own channel, viewers too, since their
//! name and photo stand beside their comments; administrators moderate others' in `admin`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::channel::NAME_LIMIT;
use crate::commands::site::Nothing;
use crate::records::ChannelCard;
use crate::role::Role;

/// The most characters a channel's description has.
pub const DESCRIPTION_LIMIT: usize = 1000;
/// The most bytes the file of a channel's photo has.
pub const PHOTO_BYTES: usize = 5 << 20;

/// What changes about the caller's channel; what is left out stays.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Customization {
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

/// A new photo for the caller's channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewPhoto {
    /// The file, JPEG, PNG or WebP of at most [`PHOTO_BYTES`] bytes, in base64 (RFC 4648, with
    /// or without padding). Its centre is cut square.
    #[schemars(length(min = 1, max = PHOTO_BYTES.div_ceil(3) * 4))]
    pub image: String,
}

/// Change the name or the description of the caller's channel.
pub struct Update;

impl Command for Update {
    const ID: &'static str = "channel.update";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str =
        "Change the name or the description of your channel; its handle stays.";
    type Input = Customization;
    type Output = Profile;
    const WRITES: bool = true;
}

/// Give the caller's channel a photo, in place of the one it had.
pub struct SetPhoto;

impl Command for SetPhoto {
    const ID: &'static str = "channel.set_photo";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str = "Give your channel a photo: a JPEG, PNG or WebP file of at most 5 MB, in base64, cut square at its centre.";
    type Input = NewPhoto;
    type Output = Profile;
    const WRITES: bool = true;
}

/// Take the photo of the caller's channel away; its initial shows again.
pub struct RemovePhoto;

impl Command for RemovePhoto {
    const ID: &'static str = "channel.remove_photo";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str = "Remove the photo of your channel; its initial shows instead.";
    type Input = Nothing;
    type Output = Profile;
    const WRITES: bool = true;
}
