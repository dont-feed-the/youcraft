//! Where the YouCraft project lives beyond the site: its chat on Discord and its source code on
//! GitHub. The guide lists them in its footer on wide screens; the menu lists them at every size.

use crate::icons::Icon;
use crate::lang::tr;

/// The project's Discord server, where people who use and build YouCraft talk.
pub const DISCORD: &str = "https://discord.com/channels/1567312862064541858/1567312862534176780";
/// The project's repository, where its source code lives.
pub const GITHUB: &str = "https://github.com/dont-feed-the/youcraft/";

/// One place of the project, its label already in the page's language.
#[derive(Clone, Copy, Debug)]
pub struct Place {
    /// Its address.
    pub href: &'static str,
    /// The icon the menu shows beside it.
    pub icon: Icon,
    /// What it is called.
    pub label: &'static str,
}

/// The project's places, in the order they are listed.
pub fn places() -> [Place; 2] {
    [
        Place {
            href: DISCORD,
            icon: Icon::Chat,
            label: tr("Chat on Discord"),
        },
        Place {
            href: GITHUB,
            icon: Icon::Code,
            label: tr("Source code on GitHub"),
        },
    ]
}
