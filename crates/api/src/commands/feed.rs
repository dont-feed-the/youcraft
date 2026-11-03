//! Commands that list videos to watch.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::records::Feed;
use crate::role::Role;

/// Where a feed starts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
}
