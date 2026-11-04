//! Commands of the studio, where a channel's members manage its videos: any member sees them,
//! editors and above change them (see `video`).

use crate::command::Command;
use crate::commands::channel::Shelf;
use crate::commands::video::Which;
use crate::records::{StudioPage, StudioVideo};
use crate::role::Role;

/// A channel's videos, newest first, in every state.
pub struct Videos;

impl Command for Videos {
    const ID: &'static str = "studio.videos";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str = "List the videos of a channel you belong to, newest first, with their visibility and processing state.";
    type Input = Shelf;
    type Output = StudioPage;
    const WRITES: bool = false;
}

/// A video of a channel the caller belongs to, in whatever state it is.
pub struct Video;

impl Command for Video {
    const ID: &'static str = "studio.video";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str = "Get a video of a channel you belong to, with its visibility and processing state, as the studio lists it.";
    type Input = Which;
    type Output = StudioVideo;
    const WRITES: bool = false;
}
