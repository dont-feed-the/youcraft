//! Commands about the signed-in person's own account: who they are, the channels they run and
//! the one they act as, their preferences, signing out.
//!
//! A person acts on the site as one of their channels at a time: it comments, likes, subscribes,
//! keeps playlists and a history. Each credential (a browser's session, a token) remembers which
//! channel it acts as; until its person chooses, or once they no longer manage it, it acts as the
//! first channel they manage, and as none when they manage none.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::site::Nothing;
use crate::ids::{AccountId, ChannelId};
use crate::records::ChannelCard;
use crate::role::{ChannelRole, Grant, Role};

/// How pages are drawn: as the device prefers, or always dark or light.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// As the device prefers.
    #[default]
    Device,
    /// Dark.
    Dark,
    /// Light.
    Light,
}

impl Theme {
    /// Every theme, in the order a menu lists them.
    pub const ALL: [Theme; 3] = [Theme::Device, Theme::Dark, Theme::Light];

    /// The name the database, cookies and `<html data-theme>` use.
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::Device => "device",
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }

    /// The theme named `name`, as [`Theme::as_str`] writes it.
    pub fn from_name(name: &str) -> Option<Theme> {
        Theme::ALL.into_iter().find(|theme| theme.as_str() == name)
    }
}

/// What a person chose about how the site looks to them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    /// The language's code (`fr`, `pt-BR`), or `None` for their browser's.
    pub language: Option<String>,
    /// The theme.
    pub theme: Theme,
}

/// A channel a person belongs to, and what they are to it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Membership {
    /// The channel.
    pub channel: ChannelCard,
    /// Their role in it.
    pub role: ChannelRole,
}

/// The signed-in person, as they see themselves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Me {
    /// Their account.
    pub account: AccountId,
    /// Their name, as Google gave it when they first signed in.
    pub name: String,
    /// The channel they act as; `None` when they manage none, and then they act toward no one
    /// until they make one.
    pub channel: Option<ChannelCard>,
    /// Every channel they belong to, the one they joined first first.
    pub channels: Vec<Membership>,
    /// How many invitations to run a channel wait for their answer.
    pub invitations: u64,
    /// The role their account holds, raised to what the site grants every account that is not
    /// blocked.
    pub role: Grant,
    /// Whether the site's administrators blocked them: they may look, and manage and take back
    /// what is theirs, but not act toward others.
    pub blocked: bool,
    /// Their preferences.
    pub preferences: Preferences,
    /// Whether the channel they act as paused its watch history, which then keeps nothing it
    /// watches.
    #[serde(default)]
    pub history_paused: bool,
}

/// The channel to act as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Acting {
    /// A channel the caller manages or owns.
    pub channel: ChannelId,
}

/// Who is signed in.
pub struct Get;

impl Command for Get {
    const ID: &'static str = "account.me";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str = "Tell who is signed in, with the channel they act as, their channels, role and preferences; null when nobody is.";
    type Input = Nothing;
    type Output = Option<Me>;
    const WRITES: bool = false;
}

/// Act as another of one's channels, with the credential that sends this command.
pub struct ActAs;

impl Command for ActAs {
    const ID: &'static str = "account.act_as";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str = "Act as another channel you manage or own, from this browser or token on: what you then do, you do as it.";
    type Input = Acting;
    type Output = Me;
    const WRITES: bool = true;
}

/// Change the signed-in person's preferences.
pub struct SetPreferences;

impl Command for SetPreferences {
    const ID: &'static str = "account.settings";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str =
        "Set the language and theme the site uses for the signed-in person.";
    type Input = Preferences;
    type Output = Preferences;
    const WRITES: bool = true;
}

/// Sign out this browser.
pub struct SignOut;

impl Command for SignOut {
    const ID: &'static str = "account.signout";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str =
        "End the session that sends this command, signing its browser out.";
    type Input = Nothing;
    type Output = Nothing;
    const WRITES: bool = true;
}
