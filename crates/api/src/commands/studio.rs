//! Commands of the studio, where a channel manages its videos.

use crate::command::Command;
use crate::commands::feed::Page;
use crate::records::StudioPage;
use crate::role::Role;

/// The caller's videos, newest first, in every state.
pub struct Videos;

impl Command for Videos {
    const ID: &'static str = "studio.videos";
    const REQUIRES: Role = Role::Creator;
    const SUMMARY: &'static str = "List the videos of your channel, newest first, with their visibility and processing state.";
    type Input = Page;
    type Output = StudioPage;
}
