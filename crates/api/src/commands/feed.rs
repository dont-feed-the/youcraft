//! Commands that list videos to watch.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::records::Feed;
use crate::role::Role;

/// Where a feed starts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Page {
    /// The `next` of the previous page; `None` for the first.
    #[serde(default)]
    pub after: Option<String>,
}

/// The home page's videos, as the caller's recommendation algorithm orders them.
pub struct Home;

impl Command for Home {
    const ID: &'static str = "feed.home";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str =
        "List the videos of the home page, in the order the recommendation algorithm chose.";
    type Input = Page;
    type Output = Feed;
    const WRITES: bool = false;
}

/// The newest videos of the channels the caller subscribes to.
pub struct Subscriptions;

impl Command for Subscriptions {
    const ID: &'static str = "feed.subscriptions";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str = "List the newest public videos of the channels you subscribe to.";
    type Input = Page;
    type Output = Feed;
    const WRITES: bool = false;
}
