//! Commands about the signed-in person's own account: who they are, their preferences, signing
//! out.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::site::Nothing;
use crate::ids::AccountId;
use crate::records::ChannelCard;
use crate::role::{Grant, Role};

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

/// The signed-in person, as they see themselves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Me {
    /// Their account.
    pub account: AccountId,
    /// Their channel.
    pub channel: ChannelCard,
    /// The role their account holds, raised to what the site grants every account that is not
    /// blocked.
    pub role: Grant,
    /// Whether the site's administrators blocked them: they may look, and manage and take back
    /// what is theirs, but not act toward others.
    pub blocked: bool,
    /// Their preferences.
    pub preferences: Preferences,
}

/// Who is signed in.
pub struct Get;

impl Command for Get {
    const ID: &'static str = "account.me";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str =
        "Tell who is signed in, with their channel, role and preferences; null when nobody is.";
    type Input = Nothing;
    type Output = Option<Me>;
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
}
