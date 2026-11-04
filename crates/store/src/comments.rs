//! Comments and replies: reading them as their reader sees them, writing, editing, deleting and
//! liking them. The counts they change are kept by the schema's triggers.
//!
//! A comment shows wherever its video does ([`crate::videos::VISIBLE`]) unless it is hidden: its
//! author is blocked from the site or by the video's channel, or it answers a hidden comment. The
//! schema keeps `comments.hidden`, and the counts that leave hidden comments out, true as blocks
//! come and go. Each write is one transaction that checks what it may do as it does it.

use rusqlite::{OptionalExtension, Row, Transaction, named_params};
use youcraft_api::commands::comment::{Comment, Order};
use youcraft_api::{CommentId, VideoId};

use crate::channels::channel_card;
use crate::videos::VISIBLE;
use crate::{Paged, Reader, Store, StoreError, Window, count};

/// Whether the comment `m` shows: neither it nor the comment it answers is hidden.
const SHOWN: &str = "m.hidden = 0 AND (m.parent_id IS NULL OR EXISTS \
    (SELECT 1 FROM comments p WHERE p.id = m.parent_id AND p.hidden = 0))";

/// Whether `:reader` may delete the comment `m` on a video of the channel `c`: they wrote it, the
/// video is their channel's, or they run the site (`:admin`).
const DELETABLE: &str = "(m.account_id IS :reader OR c.account_id IS :reader OR :admin)";

/// The columns [`comment`] reads, from `comments` as `m` (with its author's channel `a`, its
/// video `v` and the video's channel `c`), for the reader `:reader`.
fn columns() -> String {
    format!(
        "m.id, m.video_id, m.parent_id, a.id, a.handle, a.name, m.body, m.like_count, \
         m.reply_count, m.created_at, m.edited_at IS NOT NULL, \
         EXISTS (SELECT 1 FROM comment_likes l WHERE l.comment_id = m.id AND l.account_id = :reader), \
         m.account_id = c.account_id, m.account_id IS :reader, {DELETABLE}"
    )
}

/// The comments `:reader` may see, with the tables [`columns`] reads.
fn seen() -> String {
    format!(
        "comments m JOIN channels a ON a.account_id = m.account_id JOIN videos v ON v.id = m.video_id \
         JOIN channels c ON c.id = v.channel_id WHERE {SHOWN} AND {VISIBLE}"
    )
}

fn comment(row: &Row<'_>) -> rusqlite::Result<Comment> {
    Ok(Comment {
        id: CommentId::from_i64(row.get(0)?),
        video: VideoId::from_i64(row.get(1)?),
        parent: row.get::<_, Option<i64>>(2)?.map(CommentId::from_i64),
        author: channel_card(row, 3)?,
        body: row.get(6)?,
        likes: count(row.get(7)?),
        replies: count(row.get(8)?),
        created_at: row.get(9)?,
        edited: row.get(10)?,
        liked: row.get(11)?,
        by_creator: row.get(12)?,
        editable: row.get(13)?,
        deletable: row.get(14)?,
    })
}

/// The comment `id`, as `reader` sees it, read inside `transaction`.
fn seen_comment(
    transaction: &Transaction<'_>,
    id: CommentId,
    reader: Reader,
) -> Result<Option<Comment>, StoreError> {
    let found = transaction
        .query_row(
            &format!("SELECT {} FROM {} AND m.id = :id", columns(), seen()),
            named_params! {":id": id.to_i64(), ":reader": reader.id(), ":admin": reader.admin},
            comment,
        )
        .optional()?;
    Ok(found)
}

/// What writing, changing or deleting a comment found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome<T> {
    /// Done, with what it made.
    Done(T),
    /// There is no such comment or video, or not one its writer may see; or they are signed out.
    Missing,
    /// It is not theirs to change, or the creator does not let them comment.
    Refused,
}

/// A comment to write.
pub struct NewComment<'a> {
    /// Its identifier.
    pub id: CommentId,
    /// The video.
    pub video: VideoId,
    /// The comment it answers, if it is a reply; a reply to a reply answers its top-level comment.
    pub parent: Option<CommentId>,
    /// What it says.
    pub body: &'a str,
    /// Now, in seconds since 1970.
    pub now: i64,
}

impl Store {
    /// A page of the top-level comments of `video` that `reader` may see, in `order`.
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
            "SELECT {} FROM {} AND m.video_id = :video AND m.parent_id IS NULL \
             ORDER BY {order} LIMIT :limit OFFSET :offset",
            columns(),
            seen()
        ))?;
        let rows = statement.query_map(
            named_params! {
                ":video": video.to_i64(), ":reader": reader.id(), ":admin": reader.admin,
                ":limit": window.probe(), ":offset": window.skip()
            },
            comment,
        )?;
        Ok(window.page(rows.collect::<Result<_, _>>()?))
    }

    /// A page of the replies to `parent`, oldest first, if `reader` may see it.
    pub fn replies(
        &self,
        parent: CommentId,
        reader: Reader,
        window: Window,
    ) -> Result<Option<Paged<Comment>>, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        if seen_comment(&transaction, parent, reader)?.is_none() {
            return Ok(None);
        }
        let replies = {
            let mut statement = transaction.prepare_cached(&format!(
                "SELECT {} FROM {} AND m.parent_id = :parent \
                 ORDER BY m.created_at, m.id LIMIT :limit OFFSET :offset",
                columns(),
                seen()
            ))?;
            let rows = statement.query_map(
                named_params! {
                    ":parent": parent.to_i64(), ":reader": reader.id(), ":admin": reader.admin,
                    ":limit": window.probe(), ":offset": window.skip()
                },
                comment,
            )?;
            window.page(rows.collect::<Result<_, _>>()?)
        };
        transaction.commit()?;
        Ok(Some(replies))
    }

    /// The comment `id`, as `reader` sees it.
    pub fn comment(&self, id: CommentId, reader: Reader) -> Result<Option<Comment>, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let found = seen_comment(&transaction, id, reader)?;
        transaction.commit()?;
        Ok(found)
    }

    /// Write `new` as `author`, where they may: a video they may see, whose creator lets them
    /// comment, answering a comment of that video they may see.
    pub fn add_comment(
        &self,
        author: Reader,
        new: &NewComment<'_>,
    ) -> Result<Outcome<Comment>, StoreError> {
        let Some(account) = author.account else {
            return Ok(Outcome::Missing);
        };
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let open: Option<bool> = transaction
            .query_row(
                &format!(
                    "SELECT v.comments_enabled = 1 \
                     AND NOT EXISTS (SELECT 1 FROM accounts p WHERE p.id = :reader AND p.blocked = 1) \
                     AND NOT EXISTS (SELECT 1 FROM blocks b WHERE b.channel_id = v.channel_id AND b.account_id = :reader) \
                     FROM videos v WHERE v.id = :video AND {VISIBLE}"
                ),
                named_params! {":video": new.video.to_i64(), ":reader": author.id(), ":admin": author.admin},
                |row| row.get(0),
            )
            .optional()?;
        match open {
            None => return Ok(Outcome::Missing),
            Some(false) => return Ok(Outcome::Refused),
            Some(true) => {}
        }
        let parent = match new.parent {
            None => None,
            Some(parent) => {
                let top: Option<i64> = transaction
                    .query_row(
                        &format!(
                            "SELECT COALESCE(m.parent_id, m.id) FROM comments m \
                             WHERE m.id = :parent AND m.video_id = :video AND {SHOWN}"
                        ),
                        named_params! {":parent": parent.to_i64(), ":video": new.video.to_i64()},
                        |row| row.get(0),
                    )
                    .optional()?;
                match top {
                    None => return Ok(Outcome::Missing),
                    Some(top) => Some(top),
                }
            }
        };
        transaction.execute(
            "INSERT INTO comments (id, video_id, account_id, parent_id, body, created_at) \
             VALUES (:id, :video, :account, :parent, :body, :now)",
            named_params! {
                ":id": new.id.to_i64(), ":video": new.video.to_i64(), ":account": account.to_i64(),
                ":parent": parent, ":body": new.body, ":now": new.now
            },
        )?;
        let written = seen_comment(&transaction, new.id, author)?.ok_or_else(|| {
            StoreError::Corrupt(format!("comment {} hid as it was written", new.id))
        })?;
        transaction.commit()?;
        Ok(Outcome::Done(written))
    }

    /// Make the comment `id` say `body`, if `author` wrote it and may still see it.
    pub fn edit_comment(
        &self,
        id: CommentId,
        author: Reader,
        body: &str,
        now: i64,
    ) -> Result<Outcome<Comment>, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let Some(found) = seen_comment(&transaction, id, author)? else {
            return Ok(Outcome::Missing);
        };
        if !found.editable {
            return Ok(Outcome::Refused);
        }
        transaction.execute(
            "UPDATE comments SET body = :body, edited_at = :now WHERE id = :id AND account_id = :reader",
            named_params! {":id": id.to_i64(), ":reader": author.id(), ":body": body, ":now": now},
        )?;
        let edited = seen_comment(&transaction, id, author)?;
        transaction.commit()?;
        Ok(edited.map_or(Outcome::Missing, Outcome::Done))
    }

    /// Delete the comment `id` with its replies, if `reader` may: they wrote it, it is on their
    /// channel's video, or they run the site. They may delete a comment that is hidden.
    pub fn delete_comment(&self, id: CommentId, reader: Reader) -> Result<Outcome<()>, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let deleted = transaction.execute(
            &format!(
                "DELETE FROM comments WHERE id = :id AND EXISTS (SELECT 1 FROM comments m \
                 JOIN videos v ON v.id = m.video_id JOIN channels c ON c.id = v.channel_id \
                 WHERE m.id = :id AND {DELETABLE})"
            ),
            named_params! {":id": id.to_i64(), ":reader": reader.id(), ":admin": reader.admin},
        )?;
        let outcome = if deleted > 0 {
            Outcome::Done(())
        } else if seen_comment(&transaction, id, reader)?.is_some() {
            Outcome::Refused
        } else {
            Outcome::Missing
        };
        transaction.commit()?;
        Ok(outcome)
    }

    /// Make `reader` like the comment `id`, or stop; its likes then, if they may see it.
    pub fn like_comment(
        &self,
        id: CommentId,
        reader: Reader,
        liked: bool,
    ) -> Result<Option<u64>, StoreError> {
        let Some(account) = reader.account else {
            return Ok(None);
        };
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        if seen_comment(&transaction, id, reader)?.is_none() {
            return Ok(None);
        }
        let sql = if liked {
            "INSERT OR IGNORE INTO comment_likes (account_id, comment_id) VALUES (:account, :id)"
        } else {
            "DELETE FROM comment_likes WHERE account_id = :account AND comment_id = :id"
        };
        transaction.execute(
            sql,
            named_params! {":account": account.to_i64(), ":id": id.to_i64()},
        )?;
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
    use youcraft_api::records::Visibility;
    use youcraft_api::{AccountId, ChannelId, Grant};

    use super::*;
    use crate::accounts::NewAccount;
    use crate::studio::VideoEdit;
    use crate::testing::{ADA, ADA_CHANNEL, BOB, PUBLIC, with_videos};

    /// Eve, a third person, who replies.
    const EVE: AccountId = AccountId::from_bits(5);
    const ALL: Window = Window {
        offset: 0,
        limit: 20,
    };
    const ADMIN: Reader = Reader {
        account: None,
        admin: true,
    };

    fn with_eve() -> Store {
        let store = with_videos();
        store
            .create_account(&NewAccount {
                id: EVE,
                channel: ChannelId::from_bits(6),
                google_subject: "g3",
                email: "eve@example.com",
                name: "Eve",
                handle: "eve",
                role: Grant::Viewer,
                now: 100,
            })
            .unwrap();
        store
    }

    fn post(store: &Store, id: u64, parent: Option<u64>, author: AccountId) -> Outcome<Comment> {
        let new = NewComment {
            id: CommentId::from_bits(id),
            video: PUBLIC,
            parent: parent.map(CommentId::from_bits),
            body: "Lovely.",
            now: i64::try_from(id).unwrap(),
        };
        store.add_comment(Reader::of(author), &new).unwrap()
    }

    fn write(store: &Store, id: u64, parent: Option<u64>, author: AccountId) {
        let posted = post(store, id, parent, author);
        assert!(matches!(posted, Outcome::Done(_)), "{posted:?}");
    }

    fn top(store: &Store, reader: Reader) -> Vec<u64> {
        let page = store.comments(PUBLIC, Order::Newest, reader, ALL).unwrap();
        page.items.iter().map(|c| c.id.bits()).collect()
    }

    fn counted(store: &Store) -> u64 {
        let page = store.video_page(PUBLIC, Reader::ANYONE).unwrap().unwrap();
        page.comments
    }

    #[test]
    fn replies_stay_one_level_deep_and_counts_follow() {
        let store = with_videos();
        write(&store, 1, None, BOB);
        write(&store, 2, None, ADA);
        write(&store, 3, Some(1), ADA);
        write(&store, 4, Some(3), BOB);
        let bob = Reader::of(BOB);
        let best = store.comments(PUBLIC, Order::Top, bob, ALL).unwrap();
        let ids: Vec<u64> = best.items.iter().map(|c| c.id.bits()).collect();
        assert_eq!(ids, [1, 2], "the one with replies first");
        assert_eq!(
            best.items[0].replies, 2,
            "a reply to a reply answers the comment"
        );
        let newest = store.comments(PUBLIC, Order::Newest, bob, ALL).unwrap();
        assert_eq!(newest.items[0].id.bits(), 2);
        assert!(newest.items[0].by_creator && !newest.items[0].editable);
        let replies = store
            .replies(CommentId::from_bits(1), bob, ALL)
            .unwrap()
            .unwrap();
        let ids: Vec<u64> = replies.items.iter().map(|c| c.id.bits()).collect();
        assert_eq!(ids, [3, 4]);
        assert_eq!(counted(&store), 4);
        assert_eq!(
            post(&store, 5, Some(99), BOB),
            Outcome::Missing,
            "no such comment to answer"
        );
        assert_eq!(
            store
                .delete_comment(CommentId::from_bits(1), Reader::of(ADA))
                .unwrap(),
            Outcome::Done(()),
            "the creator deletes any comment on her video"
        );
        assert_eq!(counted(&store), 1);
        assert_eq!(
            store.replies(CommentId::from_bits(1), bob, ALL).unwrap(),
            None
        );
    }

    #[test]
    fn only_the_author_edits_and_likes_count_once() {
        let store = with_videos();
        write(&store, 1, None, BOB);
        let id = CommentId::from_bits(1);
        let (ada, bob) = (Reader::of(ADA), Reader::of(BOB));
        assert_eq!(
            store.edit_comment(id, ada, "Mine now", 11).unwrap(),
            Outcome::Refused
        );
        let Outcome::Done(edited) = store.edit_comment(id, bob, "Lovely!", 11).unwrap() else {
            panic!("Bob edits his own comment");
        };
        assert!(edited.edited && edited.body == "Lovely!");
        assert_eq!(store.like_comment(id, ada, true).unwrap(), Some(1));
        assert_eq!(store.like_comment(id, ada, true).unwrap(), Some(1));
        let seen = store.comment(id, ada).unwrap().unwrap();
        assert!(seen.edited && seen.liked && seen.deletable && !seen.editable);
        assert_eq!(store.like_comment(id, ada, false).unwrap(), Some(0));
        assert_eq!(store.like_comment(id, Reader::ANYONE, true).unwrap(), None);
        assert_eq!(
            store.delete_comment(id, Reader::ANYONE).unwrap(),
            Outcome::Refused
        );
        assert_eq!(
            store
                .delete_comment(CommentId::from_bits(9), Reader::of(ADA))
                .unwrap(),
            Outcome::Missing
        );
    }

    /// A blocked author's comments, and the replies to them, show to nobody and count nowhere;
    /// they come back with their counts when the block is lifted.
    #[test]
    fn a_blocked_author_neither_comments_nor_shows_nor_counts() {
        let store = with_eve();
        write(&store, 1, None, BOB);
        write(&store, 2, Some(1), EVE);
        write(&store, 3, Some(1), BOB);
        write(&store, 4, None, EVE);
        write(&store, 5, Some(4), BOB);
        assert_eq!(counted(&store), 5);
        let eve = Reader::of(EVE);

        store.set_channel_block(ADA_CHANNEL, BOB, true, 11).unwrap();
        assert_eq!(post(&store, 6, None, BOB), Outcome::Refused);
        assert_eq!(top(&store, eve), [4]);
        assert_eq!(counted(&store), 1, "Bob's thread and his reply are gone");
        let eves = store
            .comment(CommentId::from_bits(4), eve)
            .unwrap()
            .unwrap();
        assert_eq!(eves.replies, 0);
        assert_eq!(
            store.replies(CommentId::from_bits(1), eve, ALL).unwrap(),
            None
        );
        assert_eq!(store.comment(CommentId::from_bits(2), eve).unwrap(), None);
        store
            .set_channel_block(ADA_CHANNEL, BOB, false, 12)
            .unwrap();
        assert_eq!(counted(&store), 5);
        assert_eq!(top(&store, eve), [4, 1]);

        store.set_blocked(EVE, true).unwrap();
        assert_eq!(
            counted(&store),
            2,
            "Eve's comment, its reply and her reply are gone"
        );
        assert_eq!(top(&store, Reader::of(BOB)), [1]);
        let bobs = store
            .comment(CommentId::from_bits(1), Reader::ANYONE)
            .unwrap()
            .unwrap();
        assert_eq!(bobs.replies, 1);
        assert_eq!(post(&store, 7, Some(1), EVE), Outcome::Refused);
        assert_eq!(
            store
                .delete_comment(CommentId::from_bits(4), ADMIN)
                .unwrap(),
            Outcome::Done(()),
            "administrators delete hidden comments"
        );
        assert_eq!(
            store
                .delete_comment(CommentId::from_bits(2), Reader::of(ADA))
                .unwrap(),
            Outcome::Done(()),
            "so does the creator of the video"
        );
        store.set_blocked(EVE, false).unwrap();
        assert_eq!(counted(&store), 2);
        assert_eq!(top(&store, Reader::ANYONE), [1]);
        let total: i64 = store
            .connection()
            .query_row("SELECT count(*) FROM comments", [], |row| row.get(0))
            .unwrap();
        assert_eq!(total, 2, "nothing lost but what was deleted");

        store
            .edit_video(
                PUBLIC,
                &VideoEdit {
                    comments_enabled: Some(false),
                    ..VideoEdit::default()
                },
                13,
            )
            .unwrap();
        assert_eq!(post(&store, 8, None, BOB), Outcome::Refused);
    }

    #[test]
    fn the_comments_of_a_private_video_show_only_to_who_manages_it() {
        let store = with_videos();
        write(&store, 1, None, BOB);
        write(&store, 2, Some(1), ADA);
        let private = VideoEdit {
            visibility: Some(Visibility::Private),
            ..VideoEdit::default()
        };
        store.edit_video(PUBLIC, &private, 11).unwrap();
        let (bob, ada) = (Reader::of(BOB), Reader::of(ADA));
        let id = CommentId::from_bits(1);
        assert!(top(&store, bob).is_empty());
        assert_eq!(store.comment(id, bob).unwrap(), None);
        assert_eq!(store.replies(id, bob, ALL).unwrap(), None);
        assert_eq!(store.like_comment(id, bob, true).unwrap(), None);
        assert_eq!(post(&store, 3, None, BOB), Outcome::Missing);
        assert_eq!(
            store.edit_comment(id, bob, "Gone?", 12).unwrap(),
            Outcome::Missing
        );
        assert_eq!(top(&store, ada), [1]);
        assert_eq!(top(&store, ADMIN), [1]);
        assert_eq!(
            store.delete_comment(id, bob).unwrap(),
            Outcome::Done(()),
            "Bob still deletes what he wrote"
        );
    }
}
