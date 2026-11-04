//! Commands about comments: reading them, writing, editing and deleting them, liking them.
//!
//! A comment answers a video; a reply answers a comment, one level deep as on YouTube. Anyone
//! signed in and not blocked may comment where the video's creator lets them; the author edits
//! their own and deletes it, blocked or not, and the video's creator and the site's
//! administrators delete any.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::site::Nothing;
use crate::ids::{CommentId, VideoId};
use crate::records::ChannelCard;
use crate::role::Role;

/// The longest comment, in characters, as on YouTube.
pub const BODY_LIMIT: usize = 10_000;

/// A comment or a reply, as the caller sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Comment {
    /// Its identifier.
    pub id: CommentId,
    /// The video it is about.
    pub video: VideoId,
    /// The comment it answers, if it is a reply.
    pub parent: Option<CommentId>,
    /// Its author's channel.
    pub author: ChannelCard,
    /// What it says.
    pub body: String,
    /// How many people liked it.
    pub likes: u64,
    /// How many of its replies show; zero for a reply.
    pub replies: u64,
    /// When it was written, in seconds since 1970 (UTC).
    pub created_at: i64,
    /// Whether its author changed it since.
    pub edited: bool,
    /// Whether the caller liked it.
    pub liked: bool,
    /// Whether its author is the video's creator.
    pub by_creator: bool,
    /// Whether the caller may edit it: they wrote it.
    pub editable: bool,
    /// Whether the caller may delete it: they wrote it, made the video, or run the site.
    pub deletable: bool,
}

/// A page of comments, and where the next one starts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Comments {
    /// The comments, in the order asked for.
    pub comments: Vec<Comment>,
    /// What to send for the next page; `None` at the end.
    pub next: Option<String>,
}

/// The order of a video's comments.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// The most liked first.
    #[default]
    Top,
    /// The newest first.
    Newest,
}

/// Which comments of a video.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OfVideo {
    /// The video.
    pub video: VideoId,
    /// Their order.
    #[serde(default)]
    pub order: Order,
    /// The `next` of the previous page; `None` for the first.
    #[serde(default)]
    pub after: Option<String>,
}

/// Which replies of a comment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OfComment {
    /// The comment.
    pub comment: CommentId,
    /// The `next` of the previous page; `None` for the first.
    #[serde(default)]
    pub after: Option<String>,
}

/// A new comment, or a reply.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewComment {
    /// The video it is about.
    pub video: VideoId,
    /// The comment it answers, for a reply; a reply to a reply answers the same comment.
    #[serde(default)]
    pub parent: Option<CommentId>,
    /// What it says: 1 to [`BODY_LIMIT`] characters.
    #[schemars(length(min = 1, max = BODY_LIMIT))]
    pub body: String,
}

/// A comment's new text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    /// The comment.
    pub comment: CommentId,
    /// What it says now: 1 to [`BODY_LIMIT`] characters.
    #[schemars(length(min = 1, max = BODY_LIMIT))]
    pub body: String,
}

/// Which comment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Which {
    /// The comment.
    pub comment: CommentId,
}

/// Whether the caller likes a comment from now on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Liking {
    /// The comment.
    pub comment: CommentId,
    /// `true` to like it, `false` to take the like back.
    pub liked: bool,
}

/// A comment's likes once the caller liked it or took the like back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Liked {
    /// How many people like it now.
    pub likes: u64,
    /// Whether the caller does.
    pub liked: bool,
}

/// A video's comments.
pub struct List;

impl Command for List {
    const ID: &'static str = "comment.list";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str =
        "List the comments of a video, the most liked or the newest first, without their replies.";
    type Input = OfVideo;
    type Output = Comments;
    const WRITES: bool = false;
}

/// A comment's replies, oldest first.
pub struct Replies;

impl Command for Replies {
    const ID: &'static str = "comment.replies";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str = "List the replies to a comment, oldest first.";
    type Input = OfComment;
    type Output = Comments;
    const WRITES: bool = false;
}

/// Comment on a video, or reply to a comment.
pub struct Create;

impl Command for Create {
    const ID: &'static str = "comment.create";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str = "Comment on a video, or reply to one of its comments.";
    type Input = NewComment;
    type Output = Comment;
    const WRITES: bool = true;
}

/// Change the text of one's own comment.
pub struct Update;

impl Command for Update {
    const ID: &'static str = "comment.edit";
    const REQUIRES: Role = Role::Viewer;
    const SUMMARY: &'static str = "Change what one of your comments says.";
    type Input = Edit;
    type Output = Comment;
    const WRITES: bool = true;
}

/// Delete a comment, with its replies; blocked or not, what one wrote is one's own to take back.
pub struct Delete;

impl Command for Delete {
    const ID: &'static str = "comment.delete";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str = "Delete a comment with its replies: your own, any on your videos, or any at all for administrators.";
    type Input = Which;
    type Output = Nothing;
    const WRITES: bool = true;
}

/// Like a comment, or take the like back; a blocked account may only take it back.
pub struct Like;

impl Command for Like {
    const ID: &'static str = "comment.like";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str = "Like a comment or take your like back; answers its likes.";
    type Input = Liking;
    type Output = Liked;
    const WRITES: bool = true;
}
