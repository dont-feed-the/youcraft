//! Commands of the studio, where a channel manages its videos.

use crate::command::Command;
use crate::commands::feed::Page;
use crate::commands::video::Which;
use crate::records::{StudioPage, StudioVideo};
use crate::role::Role;

/// The caller's videos, newest first, in every state.
pub struct Videos;

impl Command for Videos {
    const ID: &'static str = "studio.videos";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str = "List the videos of your channel, newest first, with their visibility and processing state.";
    type Input = Page;
    type Output = StudioPage;
}

/// One of the caller's videos, in whatever state it is.
pub struct Video;

impl Command for Video {
    const ID: &'static str = "studio.video";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str =
        "Get one of your videos with its visibility and processing state, as the studio lists it.";
    type Input = Which;
    type Output = StudioVideo;
}
