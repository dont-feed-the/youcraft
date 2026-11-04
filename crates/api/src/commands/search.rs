//! Searching the site.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::channel::ChannelPage;
use crate::records::VideoCard;
use crate::role::Role;

/// The longest query, in characters.
pub const QUERY_LIMIT: usize = 200;

/// What to search for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Query {
    /// The words, in any script; diacritics and case do not matter: 1 to [`QUERY_LIMIT`]
    /// characters, on one line.
    #[schemars(length(min = 1, max = QUERY_LIMIT))]
    pub query: String,
    /// The `next` of the previous page; `None` for the first.
    #[serde(default)]
    pub after: Option<String>,
}

/// What a search found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Found {
    /// The channels whose name or handle holds the query, on the first page only.
    pub channels: Vec<ChannelPage>,
    /// The public videos whose title, description or channel holds the query, the best first.
    pub videos: Vec<VideoCard>,
    /// What to send for the next page; `None` at the end.
    pub next: Option<String>,
}

/// Search the site's public videos and its channels.
pub struct Search;

impl Command for Search {
    const ID: &'static str = "search.videos";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str =
        "Search the public videos by title, description and channel, and the channels by name.";
    type Input = Query;
    type Output = Found;
}
