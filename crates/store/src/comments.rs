//! Comments and replies: reading them as a viewer sees them, writing, editing, deleting and
//! liking them. The counts they change are kept by the schema's triggers.
//!
//! The comments of an account its channel's creator blocked, or the site's administrators did,
//! show to nobody.

use rusqlite::{OptionalExtension, Row, params};
use youcraft_api::commands::comment::{Comment, Order};
use youcraft_api::records::ChannelCard;
use youcraft_api::{AccountId, ChannelId, CommentId, VideoId};

use crate::videos::count;
use crate::{Paged, Store, StoreError, Window};

/// Who reads comments: their account, and whether they run the site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reader {
    /// Their account, if they are signed in.
    pub account: Option<AccountId>,
    /// Whether they are an administrator, who may delete any comment.
    pub admin: bool,
}

/// The columns [`comment`] reads, from `comments` as `m` (with its author's channel `a`, its
/// video `v` and the video's channel `c`), for the reader `?1`.
const COLUMNS: &str = "m.id, m.video_id, m.parent_id, a.id, a.handle, a.name, m.body, m.like_count, \
    m.reply_count, m.created_at, m.edited_at IS NOT NULL, \
    EXISTS (SELECT 1 FROM comment_likes l WHERE l.comment_id = m.id AND l.account_id = ?1), \
    m.account_id = c.account_id, m.account_id IS ?1, c.account_id IS ?1";

/// The tables [`COLUMNS`] reads, and the comments nobody may see left out.
const FROM: &str = "comments m JOIN channels a ON a.account_id = m.account_id \
    JOIN accounts p ON p.id = m.account_id JOIN videos v ON v.id = m.video_id \
    JOIN channels c ON c.id = v.channel_id \
    WHERE p.blocked = 0 AND NOT EXISTS (SELECT 1 FROM blocks b WHERE b.channel_id = v.channel_id AND b.account_id = m.account_id)";

fn comment(row: &Row<'_>, admin: bool) -> rusqlite::Result<Comment> {
    let mine: bool = row.get(13)?;
    let on_my_video: bool = row.get(14)?;
    Ok(Comment {
        id: CommentId::from_i64(row.get(0)?),
        video: VideoId::from_i64(row.get(1)?),
        parent: row.get::<_, Option<i64>>(2)?.map(CommentId::from_i64),
        author: ChannelCard {
            id: ChannelId::from_i64(row.get(3)?),
            handle: row.get(4)?,
            name: row.get(5)?,
        },
        body: row.get(6)?,
        likes: count(row.get(7)?),
        replies: count(row.get(8)?),
        created_at: row.get(9)?,
        edited: row.get(10)?,
        liked: row.get(11)?,
        by_creator: row.get(12)?,
        editable: mine,
        deletable: mine || on_my_video || admin,
    })
}

/// Where a new comment would go, as its author finds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opening {
    /// They may comment; a reply answers this top-level comment.
    Open {
        /// The comment a reply answers, if it is one.
        parent: Option<CommentId>,
    },
    /// The video or the comment answered is gone, or they may not see it.
    Missing,
    /// The creator turned comments off, or keeps this author from commenting.
    Closed,
}

/// A comment to write.
pub struct NewComment<'a> {
    /// Its identifier.
    pub id: CommentId,
    /// The video.
    pub video: VideoId,
    /// The top-level comment it answers, as [`Opening::Open`] gave it.
    pub parent: Option<CommentId>,
    /// Its author.
    pub author: AccountId,
    /// What it says.
    pub body: &'a str,
    /// Now, in seconds since 1970.
    pub now: i64,
}

/// What deleting a comment found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deleted {
    /// It is gone, with its replies.
    Done,
    /// There is no such comment the reader may see.
    Missing,
    /// It is not the reader's to delete.
    NotTheirs,
}

impl Store {
    /// A page of the top-level comments of `video`, in `order`.
    pub fn comments(
        &self,
        video: VideoId,
        order: Order,
        reader: Reader,
        window: Window,
    ) -> Result<Paged<Comment>, StoreError> {
        let order = match order {
            Order::Top => "m.like_count DESC, m.reply_count DESC, m.created_at DESC, m.id",
            Order::Newest => "m.created_at DESC, m.id",
        };
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {COLUMNS} FROM {FROM} AND m.video_id = ?2 AND m.parent_id IS NULL \
             ORDER BY {order} LIMIT ?3 OFFSET ?4"
        ))?;
        let rows = statement.query_map(
            params![
                reader.account.map(AccountId::to_i64),
                video.to_i64(),
                window.probe(),
                window.skip()
            ],
            |row| comment(row, reader.admin),
        )?;
        Ok(window.page(rows.collect::<Result<_, _>>()?))
    }

    /// A page of the replies to `parent`, oldest first.
    pub fn replies(
        &self,
        parent: CommentId,
        reader: Reader,
        window: Window,
    ) -> Result<Paged<Comment>, StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {COLUMNS} FROM {FROM} AND m.parent_id = ?2 \
             ORDER BY m.created_at, m.id LIMIT ?3 OFFSET ?4"
        ))?;
        let rows = statement.query_map(
            params![
                reader.account.map(AccountId::to_i64),
                parent.to_i64(),
                window.probe(),
                window.skip()
            ],
            |row| comment(row, reader.admin),
        )?;
        Ok(window.page(rows.collect::<Result<_, _>>()?))
    }

    /// The comment `id`, as `reader` sees it.
    pub fn comment(&self, id: CommentId, reader: Reader) -> Result<Option<Comment>, StoreError> {
        let connection = self.connection();
        let mut statement =
            connection.prepare_cached(&format!("SELECT {COLUMNS} FROM {FROM} AND m.id = ?2"))?;
        let found = statement
            .query_row(
                params![reader.account.map(AccountId::to_i64), id.to_i64()],
                |row| comment(row, reader.admin),
            )
            .optional()?;
        Ok(found)
    }

    /// Whether `author` may comment on `video` (the video page already found it visible to them),
    /// answering `parent` if given: replies to a reply answer its top-level comment.
    pub fn opening(
        &self,
        video: VideoId,
        parent: Option<CommentId>,
        author: AccountId,
    ) -> Result<Opening, StoreError> {
        let connection = self.connection();
        let open: Option<bool> = connection
            .query_row(
                "SELECT v.comments_enabled = 1 AND NOT EXISTS (SELECT 1 FROM blocks b \
                 WHERE b.channel_id = v.channel_id AND b.account_id = ?2) FROM videos v WHERE v.id = ?1",
                params![video.to_i64(), author.to_i64()],
                |row| row.get(0),
            )
            .optional()?;
        match open {
            None => return Ok(Opening::Missing),
            Some(false) => return Ok(Opening::Closed),
            Some(true) => {}
        }
        let Some(parent) = parent else {
            return Ok(Opening::Open { parent: None });
        };
        let top: Option<i64> = connection
            .query_row(
                "SELECT COALESCE(parent_id, id) FROM comments WHERE id = ?1 AND video_id = ?2",
                params![parent.to_i64(), video.to_i64()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(top.map_or(Opening::Missing, |top| Opening::Open {
            parent: Some(CommentId::from_i64(top)),
        }))
    }

    /// Write `comment`.
    pub fn add_comment(&self, comment: &NewComment<'_>) -> Result<(), StoreError> {
        self.connection().execute(
            "INSERT INTO comments (id, video_id, account_id, parent_id, body, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                comment.id.to_i64(),
                comment.video.to_i64(),
                comment.author.to_i64(),
                comment.parent.map(CommentId::to_i64),
                comment.body,
                comment.now
            ],
        )?;
        Ok(())
    }

    /// Change what the comment `id` of `author` says; whether it is theirs.
    pub fn edit_comment(
        &self,
        id: CommentId,
        author: AccountId,
        body: &str,
        now: i64,
    ) -> Result<bool, StoreError> {
        let changed = self.connection().execute(
            "UPDATE comments SET body = ?3, edited_at = ?4 WHERE id = ?1 AND account_id = ?2",
            params![id.to_i64(), author.to_i64(), body, now],
        )?;
        Ok(changed > 0)
    }

    /// Delete the comment `id` with its replies, if `reader` may.
    pub fn delete_comment(&self, id: CommentId, reader: Reader) -> Result<Deleted, StoreError> {
        let Some(found) = self.comment(id, reader)? else {
            return Ok(Deleted::Missing);
        };
        if !found.deletable {
            return Ok(Deleted::NotTheirs);
        }
        self.connection()
            .execute("DELETE FROM comments WHERE id = ?1", [id.to_i64()])?;
        Ok(Deleted::Done)
    }

    /// Make `account` like the comment `id`, or stop; its likes then, if it exists.
    pub fn like_comment(
        &self,
        id: CommentId,
        account: AccountId,
        liked: bool,
    ) -> Result<Option<u64>, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let exists: Option<i64> = transaction
            .query_row(
                "SELECT id FROM comments WHERE id = ?1",
                [id.to_i64()],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            return Ok(None);
        }
        if liked {
            transaction.execute(
                "INSERT OR IGNORE INTO comment_likes (account_id, comment_id) VALUES (?1, ?2)",
                params![account.to_i64(), id.to_i64()],
            )?;
        } else {
            transaction.execute(
                "DELETE FROM comment_likes WHERE account_id = ?1 AND comment_id = ?2",
                params![account.to_i64(), id.to_i64()],
            )?;
        }
        let likes: i64 = transaction.query_row(
            "SELECT like_count FROM comments WHERE id = ?1",
            [id.to_i64()],
            |row| row.get(0),
        )?;
        transaction.commit()?;
        Ok(Some(count(likes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{ADA, ADA_CHANNEL, BOB, PUBLIC, with_videos};

    const BOB_READS: Reader = Reader {
        account: Some(BOB),
        admin: false,
    };
    const ADA_READS: Reader = Reader {
        account: Some(ADA),
        admin: false,
    };
    const ALL: Window = Window {
        offset: 0,
        limit: 20,
    };

    fn write(store: &Store, id: u64, parent: Option<CommentId>, author: AccountId, now: i64) {
        let Opening::Open { parent } = store.opening(PUBLIC, parent, author).unwrap() else {
            panic!("closed");
        };
        store
            .add_comment(&NewComment {
                id: CommentId::from_bits(id),
                video: PUBLIC,
                parent,
                author,
                body: "Lovely.",
                now,
            })
            .unwrap();
    }

    #[test]
    fn replies_stay_one_level_deep_and_counts_follow() {
        let store = with_videos();
        write(&store, 1, None, BOB, 10);
        write(&store, 2, None, ADA, 11);
        write(&store, 3, Some(CommentId::from_bits(1)), ADA, 12);
        write(&store, 4, Some(CommentId::from_bits(3)), BOB, 13);
        let top = store.comments(PUBLIC, Order::Top, BOB_READS, ALL).unwrap();
        let ids: Vec<u64> = top.items.iter().map(|c| c.id.bits()).collect();
        assert_eq!(ids, [1, 2], "the one with replies first");
        assert_eq!(
            top.items[0].replies, 2,
            "a reply to a reply answers the comment"
        );
        let newest = store
            .comments(PUBLIC, Order::Newest, BOB_READS, ALL)
            .unwrap();
        assert_eq!(newest.items[0].id.bits(), 2);
        assert!(newest.items[0].by_creator && !newest.items[0].editable);
        let replies = store
            .replies(CommentId::from_bits(1), BOB_READS, ALL)
            .unwrap();
        let ids: Vec<u64> = replies.items.iter().map(|c| c.id.bits()).collect();
        assert_eq!(ids, [3, 4]);
        let page = store.video_page(PUBLIC, None).unwrap().unwrap();
        assert_eq!(page.comments, 4);
        assert_eq!(
            store
                .delete_comment(CommentId::from_bits(1), ADA_READS)
                .unwrap(),
            Deleted::Done,
            "the creator deletes any comment on her video"
        );
        assert_eq!(store.video_page(PUBLIC, None).unwrap().unwrap().comments, 1);
    }

    #[test]
    fn only_the_author_edits_and_likes_count_once() {
        let store = with_videos();
        write(&store, 1, None, BOB, 10);
        let id = CommentId::from_bits(1);
        assert!(!store.edit_comment(id, ADA, "Mine now", 11).unwrap());
        assert!(store.edit_comment(id, BOB, "Lovely!", 11).unwrap());
        assert_eq!(store.like_comment(id, ADA, true).unwrap(), Some(1));
        assert_eq!(store.like_comment(id, ADA, true).unwrap(), Some(1));
        let seen = store.comment(id, ADA_READS).unwrap().unwrap();
        assert!(seen.edited && seen.liked && seen.deletable && !seen.editable);
        assert_eq!(seen.body, "Lovely!");
        assert_eq!(store.like_comment(id, ADA, false).unwrap(), Some(0));
        assert_eq!(
            store
                .delete_comment(
                    id,
                    Reader {
                        account: None,
                        admin: false
                    }
                )
                .unwrap(),
            Deleted::NotTheirs
        );
    }

    #[test]
    fn a_blocked_author_neither_comments_nor_shows() {
        let store = with_videos();
        write(&store, 1, None, BOB, 10);
        store.set_channel_block(ADA_CHANNEL, BOB, true, 11).unwrap();
        assert_eq!(store.opening(PUBLIC, None, BOB).unwrap(), Opening::Closed);
        assert!(
            store
                .comments(PUBLIC, Order::Top, ADA_READS, ALL)
                .unwrap()
                .items
                .is_empty()
        );
        store
            .set_channel_block(ADA_CHANNEL, BOB, false, 12)
            .unwrap();
        store
            .connection()
            .execute("UPDATE videos SET comments_enabled = 0", [])
            .unwrap();
        assert_eq!(store.opening(PUBLIC, None, BOB).unwrap(), Opening::Closed);
        assert_eq!(
            store.opening(VideoId::from_bits(99), None, BOB).unwrap(),
            Opening::Missing
        );
    }
}
