//! Commands that bring a video's file to the site.
//!
//! An upload is resumable: `upload.create` makes the video and answers where its bytes go;
//! the client sends them in pieces (`PATCH` at an offset, `HEAD` to ask the offset after a
//! break); `upload.finish` hands the whole file over to be made into renditions.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::video::Which;
use crate::records::{Processing, Upload, Visibility};
use crate::role::Role;

/// A video to upload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewUpload {
    /// Its title.
    pub title: String,
    /// Its description.
    #[serde(default)]
    pub description: String,
    /// Who may see it once it plays.
    #[serde(default)]
    pub visibility: Visibility,
    /// The size of its file in bytes.
    pub size: u64,
}

/// Start uploading a video to the caller's channel.
pub struct Create;

impl Command for Create {
    const ID: &'static str = "upload.create";
    const REQUIRES: Role = Role::Creator;
    const SUMMARY: &'static str =
        "Create a video on your channel and get the address its file is uploaded to, in pieces.";
    type Input = NewUpload;
    type Output = Upload;
}

/// Where an upload stands, to resume it.
pub struct Status;

impl Command for Status {
    const ID: &'static str = "upload.status";
    const REQUIRES: Role = Role::Creator;
    const SUMMARY: &'static str =
        "Get the address of an upload in progress and how many bytes it has received.";
    type Input = Which;
    type Output = Upload;
}

/// Hand a fully uploaded file over to be made playable.
pub struct Finish;

impl Command for Finish {
    const ID: &'static str = "upload.finish";
    const REQUIRES: Role = Role::Creator;
    const SUMMARY: &'static str =
        "Finish an upload whose bytes have all arrived; the video is then made playable.";
    type Input = Which;
    type Output = Processing;
}
