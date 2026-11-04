//! Comments: read by anyone who may see the video, written as the channel its writer acts as
//! where the video's channel lets them, edited by whoever speaks as its channel, deleted by them,
//! by whoever speaks as the video's channel, or by an administrator. The store checks each of
//! these as it reads or writes.

use youcraft_api::commands::comment::{
    BODY_LIMIT, Comment, Comments, Create, Delete, Edit, Like, Liked, Liking, List, NewComment,
    OfComment, OfVideo, Replies, Update, Which,
};
use youcraft_api::commands::site::Nothing;
use youcraft_api::{ApiError, CommentId};
use youcraft_store::comments::{self, Outcome};

use crate::dispatch::Handle;
use crate::handlers::input::{next, text, window};
use crate::secrets::random_bits;
use crate::site::{Caller, Site, now};

/// How many comments one page holds.
const PAGE: u32 = 20;

/// What a write to comments made, or why it made nothing.
fn outcome<T>(outcome: Outcome<T>) -> Result<T, ApiError> {
    match outcome {
        Outcome::Done(made) => Ok(made),
        Outcome::Missing => Err(ApiError::NotFound),
        Outcome::Refused => Err(ApiError::Forbidden),
    }
}

impl Handle<List> for Site {
    async fn handle(&self, caller: Caller, of: OfVideo) -> Result<Comments, ApiError> {
        let page = self.video_page(caller, of.video).await?;
        let window = window(of.after.as_deref(), PAGE)?;
        if !page.comments_enabled {
            return Ok(Comments {
                comments: Vec::new(),
                next: None,
            });
        }
        let reader = caller.reader();
        let found = self
            .db(move |store| store.comments(of.video, of.order, reader, window))
            .await?;
        Ok(Comments {
            comments: found.items,
            next: next(window, found.more),
        })
    }
}

impl Handle<Replies> for Site {
    async fn handle(&self, caller: Caller, of: OfComment) -> Result<Comments, ApiError> {
        let window = window(of.after.as_deref(), PAGE)?;
        let reader = caller.reader();
        let found = self
            .db(move |store| store.replies(of.comment, reader, window))
            .await?
            .ok_or(ApiError::NotFound)?;
        Ok(Comments {
            comments: found.items,
            next: next(window, found.more),
        })
    }
}

impl Handle<Create> for Site {
    async fn handle(&self, caller: Caller, new: NewComment) -> Result<Comment, ApiError> {
        caller.acting()?;
        let body = text("body", &new.body, BODY_LIMIT)?;
        let (author, id, now) = (caller.reader(), CommentId::from_bits(random_bits()), now());
        let posted = self
            .db(move |store| {
                let comment = comments::NewComment {
                    id,
                    video: new.video,
                    parent: new.parent,
                    body: &body,
                    now,
                };
                store.add_comment(author, &comment)
            })
            .await?;
        outcome(posted)
    }
}

impl Handle<Update> for Site {
    async fn handle(&self, caller: Caller, edit: Edit) -> Result<Comment, ApiError> {
        caller.signed_in()?;
        let body = text("body", &edit.body, BODY_LIMIT)?;
        let (author, now) = (caller.reader(), now());
        let edited = self
            .db(move |store| store.edit_comment(edit.comment, author, &body, now))
            .await?;
        outcome(edited)
    }
}

impl Handle<Delete> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Nothing, ApiError> {
        caller.signed_in()?;
        let reader = caller.reader();
        let deleted = self
            .db(move |store| store.delete_comment(which.comment, reader))
            .await?;
        outcome(deleted).map(|()| Nothing {})
    }
}

impl Handle<Like> for Site {
    async fn handle(&self, caller: Caller, liking: Liking) -> Result<Liked, ApiError> {
        caller.acting()?;
        if liking.liked {
            caller.may_act()?;
        }
        let reader = caller.reader();
        let likes = self
            .db(move |store| store.like_comment(liking.comment, reader, liking.liked))
            .await?
            .ok_or(ApiError::NotFound)?;
        Ok(Liked {
            likes,
            liked: liking.liked,
        })
    }
}
