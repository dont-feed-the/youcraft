//! Commands about the signed-in person's watch history, which only they see.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::feed::Page;
use crate::commands::site::Nothing;
use crate::commands::video::Which;
use crate::records::VideoCard;
use crate::role::Role;

/// A video the person watched.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Watched {
    /// The video.
    pub card: VideoCard,
    /// When they last watched it, in seconds since 1970 (UTC).
    pub watched_at: i64,
}

/// A page of the history, and whether it is being kept.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct History {
    /// The videos, the most recently watched first.
    pub videos: Vec<Watched>,
    /// What to send for the next page; `None` at the end.
    pub next: Option<String>,
    /// Whether the person paused their history: what they watch now is not kept.
    pub paused: bool,
}

/// Whether the history is kept from now on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Pausing {
    /// `true` to stop keeping it, `false` to keep it again.
    pub paused: bool,
}

/// The caller's history.
pub struct List;

impl Command for List {
    const ID: &'static str = "history.list";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str =
        "List the videos you watched, the most recent first, and whether your history is paused.";
    type Input = Page;
    type Output = History;
}

/// Take one video out of the caller's history.
pub struct Remove;

impl Command for Remove {
    const ID: &'static str = "history.remove";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str = "Take a video out of your history.";
    type Input = Which;
    type Output = Nothing;
}

/// Empty the caller's history.
pub struct Clear;

impl Command for Clear {
    const ID: &'static str = "history.clear";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str = "Take every video out of your history.";
    type Input = Nothing;
    type Output = Nothing;
}

/// Pause the caller's history, or keep it again.
pub struct Pause;

impl Command for Pause {
    const ID: &'static str = "history.pause";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str = "Stop keeping what you watch in your history, or keep it again.";
    type Input = Pausing;
    type Output = Pausing;
}
