//! Comments: read by anyone who may see the video, written by people signed in where the
//! creator lets them, edited by their author, deleted by their author, the video's creator or
//! an administrator.

use youcraft_api::commands::comment::{
    BODY_LIMIT, Comment, Comments, Create, Delete, Edit, Like, Liked, Liking, List, NewComment,
    OfComment, OfVideo, Replies, Update, Which,
};
use youcraft_api::commands::site::Nothing;
use youcraft_api::{ApiError, CommentId, Role};
use youcraft_store::comments::{self, Deleted, Opening, Reader};

use crate::dispatch::Handle;
use crate::handlers::input::{next, text, window};
use crate::secrets::random_bits;
use crate::site::{Caller, Site, now};

/// How many comments one page holds.
const PAGE: u32 = 20;

fn reader(caller: Caller) -> Reader {
    Reader {
        account: caller.account,
        admin: caller.role >= Role::Admin,
    }
}

impl Site {
    /// The comment `id`, if `caller` may see it and its video.
    async fn seen_comment(&self, caller: Caller, id: CommentId) -> Result<Comment, ApiError> {
        let reader = reader(caller);
        let comment = self
            .db(move |store| store.comment(id, reader))
            .await?
            .ok_or(ApiError::NotFound)?;
        self.video_page(caller, comment.video).await?;
        Ok(comment)
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
        let reader = reader(caller);
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
        self.seen_comment(caller, of.comment).await?;
        let window = window(of.after.as_deref(), PAGE)?;
        let reader = reader(caller);
        let found = self
            .db(move |store| store.replies(of.comment, reader, window))
            .await?;
        Ok(Comments {
            comments: found.items,
            next: next(window, found.more),
        })
    }
}

impl Handle<Create> for Site {
    async fn handle(&self, caller: Caller, new: NewComment) -> Result<Comment, ApiError> {
        let author = caller.signed_in()?;
        let body = text("body", &new.body, BODY_LIMIT, true)?;
        self.video_page(caller, new.video).await?;
        let video = new.video;
        let opening = self
            .db(move |store| store.opening(video, new.parent, author))
            .await?;
        let parent = match opening {
            Opening::Open { parent } => parent,
            Opening::Missing => return Err(ApiError::NotFound),
            Opening::Closed => return Err(ApiError::Forbidden),
        };
        let id = CommentId::from_bits(random_bits());
        let now = now();
        self.db(move |store| {
            store.add_comment(&comments::NewComment {
                id,
                video,
                parent,
                author,
                body: &body,
                now,
            })
        })
        .await?;
        let reader = reader(caller);
        self.db(move |store| store.comment(id, reader))
            .await?
            .ok_or(ApiError::Internal)
    }
}

impl Handle<Update> for Site {
    async fn handle(&self, caller: Caller, edit: Edit) -> Result<Comment, ApiError> {
        let author = caller.signed_in()?;
        let body = text("body", &edit.body, BODY_LIMIT, true)?;
        let found = self.seen_comment(caller, edit.comment).await?;
        if !found.editable {
            return Err(ApiError::Forbidden);
        }
        let (id, now) = (edit.comment, now());
        self.db(move |store| store.edit_comment(id, author, &body, now))
            .await?;
        let reader = reader(caller);
        self.db(move |store| store.comment(id, reader))
            .await?
            .ok_or(ApiError::NotFound)
    }
}

impl Handle<Delete> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Nothing, ApiError> {
        caller.signed_in()?;
        let reader = reader(caller);
        let deleted = self
            .db(move |store| store.delete_comment(which.comment, reader))
            .await?;
        match deleted {
            Deleted::Done => Ok(Nothing {}),
            Deleted::Missing => Err(ApiError::NotFound),
            Deleted::NotTheirs => Err(ApiError::Forbidden),
        }
    }
}

impl Handle<Like> for Site {
    async fn handle(&self, caller: Caller, liking: Liking) -> Result<Liked, ApiError> {
        let account = caller.signed_in()?;
        self.seen_comment(caller, liking.comment).await?;
        let likes = self
            .db(move |store| store.like_comment(liking.comment, account, liking.liked))
            .await?
            .ok_or(ApiError::NotFound)?;
        Ok(Liked {
            likes,
            liked: liking.liked,
        })
    }
}
