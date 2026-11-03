//! Lists of videos: playlists, each account's Watch later list, and its liked videos, which the
//! `likes` table already holds. A list shows only the videos its reader may see.

use rusqlite::{OptionalExtension, Row, Transaction, params};
use youcraft_api::commands::playlist::{PLAYLIST_LIMIT, PlaylistCard, VIDEO_LIMIT};
use youcraft_api::records::{ChannelCard, VideoCard, Visibility};
use youcraft_api::{AccountId, ChannelId, List, PlaylistId, VideoId};

use crate::videos::{CARD_COLUMNS, card, count};
use crate::watching::visible_to;
use crate::{Paged, Store, StoreError, Window};

/// The columns [`playlist_card`] reads, from `playlists` as `p` joined with its owner's channel
/// `o`, telling whether it holds the video `?1`.
const CARD: &str = "p.id, p.kind, p.title, p.visibility, p.updated_at, o.id, o.handle, o.name, \
    (SELECT count(*) FROM playlist_items i WHERE i.playlist_id = p.id), \
    (SELECT i.video_id FROM playlist_items i WHERE i.playlist_id = p.id ORDER BY i.position LIMIT 1), \
    EXISTS (SELECT 1 FROM playlist_items i WHERE i.playlist_id = p.id AND i.video_id = ?1)";

/// The tables [`CARD`] reads.
const FROM: &str = "playlists p JOIN channels o ON o.account_id = p.account_id";

fn owner(row: &Row<'_>, at: usize) -> rusqlite::Result<ChannelCard> {
    Ok(ChannelCard {
        id: ChannelId::from_i64(row.get(at)?),
        handle: row.get(at + 1)?,
        name: row.get(at + 2)?,
    })
}

fn playlist_card(row: &Row<'_>) -> rusqlite::Result<PlaylistCard> {
    let kind: String = row.get(1)?;
    let list = if kind == "watch_later" {
        List::WatchLater
    } else {
        List::Playlist(PlaylistId::from_i64(row.get(0)?))
    };
    let visibility: String = row.get(3)?;
    Ok(PlaylistCard {
        list,
        title: row.get(2)?,
        visibility: Visibility::from_name(&visibility).unwrap_or_default(),
        updated_at: row.get(4)?,
        owner: owner(row, 5)?,
        videos: count(row.get(8)?),
        first: row.get::<_, Option<i64>>(9)?.map(VideoId::from_i64),
        contains: row.get(10)?,
    })
}

/// What changing a list found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Changed {
    /// Done: the list now.
    Done(PlaylistCard),
    /// No such list of the account's, or no such video they may see.
    Missing,
    /// The list holds [`VIDEO_LIMIT`] videos, or the account [`PLAYLIST_LIMIT`] playlists.
    Full,
    /// Liked videos changes only as its owner rates videos.
    Fixed,
}

/// What a list's page shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListPage {
    /// The list.
    pub card: PlaylistCard,
    /// What its owner says about it.
    pub description: String,
    /// A page of its videos the reader may see.
    pub videos: Paged<VideoCard>,
    /// Whether the reader may change it.
    pub editable: bool,
}

/// A playlist to make.
pub struct NewPlaylist<'a> {
    /// Its identifier.
    pub id: PlaylistId,
    /// Its owner.
    pub owner: AccountId,
    /// Its title.
    pub title: &'a str,
    /// Who may see it.
    pub visibility: Visibility,
    /// A first video to put in it.
    pub video: Option<VideoId>,
    /// Now, in seconds since 1970.
    pub now: i64,
}

/// What to change about a playlist; what is `None` stays.
pub struct PlaylistEdit<'a> {
    /// Its new title.
    pub title: Option<&'a str>,
    /// Its new description.
    pub description: Option<&'a str>,
    /// Who may now see it.
    pub visibility: Option<Visibility>,
}

/// The card of the playlist row `id`, telling whether it holds `containing`.
fn card_of(
    transaction: &Transaction<'_>,
    id: i64,
    containing: Option<VideoId>,
) -> Result<Option<PlaylistCard>, StoreError> {
    let found = transaction
        .query_row(
            &format!("SELECT {CARD} FROM {FROM} WHERE p.id = ?2"),
            params![containing.map(VideoId::to_i64), id],
            playlist_card,
        )
        .optional()?;
    Ok(found)
}

/// The card of the liked videos of `account`, telling whether they hold `containing`.
fn liked_card(
    transaction: &Transaction<'_>,
    account: AccountId,
    containing: Option<VideoId>,
) -> Result<Option<PlaylistCard>, StoreError> {
    let found = transaction
        .query_row(
            "SELECT o.id, o.handle, o.name, \
             (SELECT count(*) FROM likes WHERE account_id = ?1 AND value = 1), \
             (SELECT video_id FROM likes WHERE account_id = ?1 AND value = 1 ORDER BY created_at DESC LIMIT 1), \
             (SELECT max(created_at) FROM likes WHERE account_id = ?1 AND value = 1), \
             EXISTS (SELECT 1 FROM likes WHERE account_id = ?1 AND value = 1 AND video_id = ?2) \
             FROM channels o WHERE o.account_id = ?1",
            params![account.to_i64(), containing.map(VideoId::to_i64)],
            |row| {
                Ok(PlaylistCard {
                    list: List::Liked,
                    title: String::new(),
                    visibility: Visibility::Private,
                    owner: owner(row, 0)?,
                    videos: count(row.get(3)?),
                    first: row.get::<_, Option<i64>>(4)?.map(VideoId::from_i64),
                    updated_at: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
                    contains: row.get(6)?,
                })
            },
        )
        .optional()?;
    Ok(found)
}

/// The Watch later row of `account`, made now if `make` and it has none yet.
fn watch_later(
    transaction: &Transaction<'_>,
    account: AccountId,
    now: i64,
    make: bool,
) -> Result<Option<i64>, StoreError> {
    let find = |transaction: &Transaction<'_>| {
        transaction
            .query_row(
                "SELECT id FROM playlists WHERE account_id = ?1 AND kind = 'watch_later'",
                [account.to_i64()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
    };
    if let Some(id) = find(transaction)? {
        return Ok(Some(id));
    }
    if !make {
        return Ok(None);
    }
    transaction.execute(
        "INSERT INTO playlists (account_id, kind, title, created_at, updated_at) \
         VALUES (?1, 'watch_later', '', ?2, ?2)",
        params![account.to_i64(), now],
    )?;
    Ok(find(transaction)?)
}

/// The row of `list` if it is a playlist or the Watch later list of `account`.
fn owned(
    transaction: &Transaction<'_>,
    account: AccountId,
    list: List,
    now: i64,
) -> Result<Option<i64>, StoreError> {
    match list {
        List::WatchLater => watch_later(transaction, account, now, true),
        List::Liked => Ok(None),
        List::Playlist(id) => Ok(transaction
            .query_row(
                "SELECT id FROM playlists WHERE id = ?1 AND account_id = ?2 AND kind = 'playlist'",
                params![id.to_i64(), account.to_i64()],
                |row| row.get(0),
            )
            .optional()?),
    }
}

/// The empty Watch later list of `account`, before anything was put in it.
fn empty_watch_later(
    transaction: &Transaction<'_>,
    account: AccountId,
) -> Result<Option<PlaylistCard>, StoreError> {
    let found = transaction
        .query_row(
            "SELECT o.id, o.handle, o.name FROM channels o WHERE o.account_id = ?1",
            [account.to_i64()],
            |row| owner(row, 0),
        )
        .optional()?;
    Ok(found.map(|owner| PlaylistCard {
        list: List::WatchLater,
        title: String::new(),
        visibility: Visibility::Private,
        videos: 0,
        owner,
        updated_at: 0,
        first: None,
        contains: false,
    }))
}

/// Put `video` last in the playlist row `id`, unless it holds it already.
fn append(
    transaction: &Transaction<'_>,
    id: i64,
    account: AccountId,
    video: VideoId,
    now: i64,
) -> Result<Option<Changed>, StoreError> {
    let seen: Option<i64> = transaction
        .query_row(
            &format!(
                "SELECT v.id FROM videos v JOIN channels c ON c.id = v.channel_id WHERE v.id = ?1 AND {}",
                visible_to("?2")
            ),
            params![video.to_i64(), account.to_i64()],
            |row| row.get(0),
        )
        .optional()?;
    if seen.is_none() {
        return Ok(Some(Changed::Missing));
    }
    let held: i64 = transaction.query_row(
        "SELECT count(*) FROM playlist_items WHERE playlist_id = ?1",
        [id],
        |row| row.get(0),
    )?;
    if count(held) >= VIDEO_LIMIT {
        return Ok(Some(Changed::Full));
    }
    transaction.execute(
        "INSERT OR IGNORE INTO playlist_items (playlist_id, video_id, position, added_at) \
         SELECT ?1, ?2, COALESCE(max(position) + 1, 0), ?3 FROM playlist_items WHERE playlist_id = ?1",
        params![id, video.to_i64(), now],
    )?;
    transaction.execute(
        "UPDATE playlists SET updated_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    Ok(None)
}

impl Store {
    /// The lists of `account`: Watch later, liked videos, then their playlists, the most recently
    /// changed first; each telling whether it holds `containing`.
    pub fn lists_of(
        &self,
        account: AccountId,
        containing: Option<VideoId>,
    ) -> Result<Vec<PlaylistCard>, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let mut lists = Vec::new();
        let later = match watch_later(&transaction, account, 0, false)? {
            Some(id) => card_of(&transaction, id, containing)?,
            None => empty_watch_later(&transaction, account)?,
        };
        lists.extend(later);
        lists.extend(liked_card(&transaction, account, containing)?);
        {
            let mut statement = transaction.prepare_cached(&format!(
                "SELECT {CARD} FROM {FROM} WHERE p.account_id = ?2 AND p.kind = 'playlist' \
                 ORDER BY p.updated_at DESC, p.id"
            ))?;
            let rows = statement.query_map(
                params![containing.map(VideoId::to_i64), account.to_i64()],
                playlist_card,
            )?;
            for row in rows {
                lists.push(row?);
            }
        }
        transaction.commit()?;
        Ok(lists)
    }

    /// The public playlists of `channel`, the most recently changed first.
    pub fn channel_playlists(
        &self,
        channel: ChannelId,
        window: Window,
    ) -> Result<Paged<PlaylistCard>, StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {CARD} FROM {FROM} WHERE o.id = ?2 AND p.kind = 'playlist' AND p.visibility = 'public' \
             ORDER BY p.updated_at DESC, p.id LIMIT ?3 OFFSET ?4"
        ))?;
        let rows = statement.query_map(
            params![None::<i64>, channel.to_i64(), window.probe(), window.skip()],
            playlist_card,
        )?;
        Ok(window.page(rows.collect::<Result<_, _>>()?))
    }

    /// Make `playlist`.
    pub fn create_playlist(&self, playlist: &NewPlaylist<'_>) -> Result<Changed, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let made: i64 = transaction.query_row(
            "SELECT count(*) FROM playlists WHERE account_id = ?1 AND kind = 'playlist'",
            [playlist.owner.to_i64()],
            |row| row.get(0),
        )?;
        if count(made) >= PLAYLIST_LIMIT {
            return Ok(Changed::Full);
        }
        let id = playlist.id.to_i64();
        transaction.execute(
            "INSERT INTO playlists (id, account_id, kind, title, visibility, created_at, updated_at) \
             VALUES (?1, ?2, 'playlist', ?3, ?4, ?5, ?5)",
            params![
                id,
                playlist.owner.to_i64(),
                playlist.title,
                playlist.visibility.as_str(),
                playlist.now
            ],
        )?;
        if let Some(video) = playlist.video
            && let Some(refused) = append(&transaction, id, playlist.owner, video, playlist.now)?
        {
            return Ok(refused);
        }
        let made = card_of(&transaction, id, playlist.video)?;
        transaction.commit()?;
        Ok(made.map_or(Changed::Missing, Changed::Done))
    }

    /// Change the playlist `id` of `account`.
    pub fn update_playlist(
        &self,
        id: PlaylistId,
        account: AccountId,
        edit: &PlaylistEdit<'_>,
        now: i64,
    ) -> Result<Changed, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let changed = transaction.execute(
            "UPDATE playlists SET title = COALESCE(?3, title), description = COALESCE(?4, description), \
             visibility = COALESCE(?5, visibility), updated_at = ?6 \
             WHERE id = ?1 AND account_id = ?2 AND kind = 'playlist'",
            params![
                id.to_i64(),
                account.to_i64(),
                edit.title,
                edit.description,
                edit.visibility.map(Visibility::as_str),
                now
            ],
        )?;
        if changed == 0 {
            return Ok(Changed::Missing);
        }
        let card = card_of(&transaction, id.to_i64(), None)?;
        transaction.commit()?;
        Ok(card.map_or(Changed::Missing, Changed::Done))
    }

    /// Delete the playlist `id` of `account`; whether it was theirs.
    pub fn delete_playlist(&self, id: PlaylistId, account: AccountId) -> Result<bool, StoreError> {
        let deleted = self.connection().execute(
            "DELETE FROM playlists WHERE id = ?1 AND account_id = ?2 AND kind = 'playlist'",
            params![id.to_i64(), account.to_i64()],
        )?;
        Ok(deleted > 0)
    }

    /// Put `video` last in the `list` of `account`.
    pub fn add_to_list(
        &self,
        account: AccountId,
        list: List,
        video: VideoId,
        now: i64,
    ) -> Result<Changed, StoreError> {
        if list == List::Liked {
            return Ok(Changed::Fixed);
        }
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let Some(id) = owned(&transaction, account, list, now)? else {
            return Ok(Changed::Missing);
        };
        if let Some(refused) = append(&transaction, id, account, video, now)? {
            return Ok(refused);
        }
        let card = card_of(&transaction, id, Some(video))?;
        transaction.commit()?;
        Ok(card.map_or(Changed::Missing, Changed::Done))
    }

    /// Take `video` out of the `list` of `account`.
    pub fn remove_from_list(
        &self,
        account: AccountId,
        list: List,
        video: VideoId,
        now: i64,
    ) -> Result<Changed, StoreError> {
        if list == List::Liked {
            return Ok(Changed::Fixed);
        }
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let Some(id) = owned(&transaction, account, list, now)? else {
            return Ok(Changed::Missing);
        };
        let removed = transaction.execute(
            "DELETE FROM playlist_items WHERE playlist_id = ?1 AND video_id = ?2",
            params![id, video.to_i64()],
        )?;
        if removed > 0 {
            transaction.execute(
                "UPDATE playlists SET updated_at = ?2 WHERE id = ?1",
                params![id, now],
            )?;
        }
        let card = card_of(&transaction, id, Some(video))?;
        transaction.commit()?;
        Ok(card.map_or(Changed::Missing, Changed::Done))
    }

    /// Move `video` to `position` (counted from 0, past the end meaning last) in the `list` of
    /// `account`.
    pub fn move_in_list(
        &self,
        account: AccountId,
        list: List,
        video: VideoId,
        position: u64,
        now: i64,
    ) -> Result<Changed, StoreError> {
        if list == List::Liked {
            return Ok(Changed::Fixed);
        }
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let Some(id) = owned(&transaction, account, list, now)? else {
            return Ok(Changed::Missing);
        };
        let mut order: Vec<i64> = {
            let mut statement = transaction.prepare_cached(
                "SELECT video_id FROM playlist_items WHERE playlist_id = ?1 ORDER BY position, added_at",
            )?;
            statement
                .query_map([id], |row| row.get(0))?
                .collect::<Result<_, _>>()?
        };
        let Some(from) = order.iter().position(|v| *v == video.to_i64()) else {
            return Ok(Changed::Missing);
        };
        let moved = order.remove(from);
        let to = usize::try_from(position).map_or(order.len(), |p| p.min(order.len()));
        order.insert(to, moved);
        for (place, video) in order.iter().enumerate() {
            transaction.execute(
                "UPDATE playlist_items SET position = ?3 WHERE playlist_id = ?1 AND video_id = ?2",
                params![id, video, i64::try_from(place).unwrap_or(i64::MAX)],
            )?;
        }
        transaction.execute(
            "UPDATE playlists SET updated_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        let card = card_of(&transaction, id, None)?;
        transaction.commit()?;
        Ok(card.map_or(Changed::Missing, Changed::Done))
    }

    /// The page of `list` as `reader` sees it, if they may: Watch later and liked videos are the
    /// reader's own; a playlist shows to all unless it is private.
    pub fn list_page(
        &self,
        list: List,
        reader: Option<AccountId>,
        window: Window,
    ) -> Result<Option<ListPage>, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let viewer = reader.map(AccountId::to_i64);
        let (heading, description, row) = match (list, reader) {
            (List::Liked, Some(account)) => (
                liked_card(&transaction, account, None)?,
                String::new(),
                None,
            ),
            (List::WatchLater, Some(account)) => {
                match watch_later(&transaction, account, 0, false)? {
                    Some(id) => (card_of(&transaction, id, None)?, String::new(), Some(id)),
                    None => (
                        empty_watch_later(&transaction, account)?,
                        String::new(),
                        None,
                    ),
                }
            }
            (List::Playlist(id), _) => {
                let found: Option<(String, i64, String)> = transaction
                    .query_row(
                        "SELECT description, account_id, visibility FROM playlists WHERE id = ?1 AND kind = 'playlist'",
                        [id.to_i64()],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                match found {
                    Some((description, owner, visibility))
                        if visibility != "private" || viewer == Some(owner) =>
                    {
                        (
                            card_of(&transaction, id.to_i64(), None)?,
                            description,
                            Some(id.to_i64()),
                        )
                    }
                    _ => (None, String::new(), None),
                }
            }
            (_, None) => (None, String::new(), None),
        };
        let Some(heading) = heading else {
            return Ok(None);
        };
        let rows: Vec<VideoCard> = {
            let (sql, key) = match (list, row) {
                (List::Liked, _) => (
                    format!(
                        "SELECT {CARD_COLUMNS} FROM likes l JOIN videos v ON v.id = l.video_id \
                         JOIN channels c ON c.id = v.channel_id \
                         WHERE l.account_id = ?1 AND l.value = 1 AND {} \
                         ORDER BY l.created_at DESC, v.id LIMIT ?3 OFFSET ?4",
                        visible_to("?2")
                    ),
                    viewer,
                ),
                (_, Some(id)) => (
                    format!(
                        "SELECT {CARD_COLUMNS} FROM playlist_items i JOIN videos v ON v.id = i.video_id \
                         JOIN channels c ON c.id = v.channel_id WHERE i.playlist_id = ?1 AND {} \
                         ORDER BY i.position, i.added_at LIMIT ?3 OFFSET ?4",
                        visible_to("?2")
                    ),
                    Some(id),
                ),
                (_, None) => (String::new(), None),
            };
            match key {
                Some(key) => {
                    let mut statement = transaction.prepare_cached(&sql)?;
                    statement
                        .query_map(params![key, viewer, window.probe(), window.skip()], card)?
                        .collect::<Result<_, _>>()?
                }
                None => Vec::new(),
            }
        };
        let readers_channel: Option<i64> = transaction
            .query_row(
                "SELECT id FROM channels WHERE account_id = ?1",
                [viewer],
                |row| row.get(0),
            )
            .optional()?;
        transaction.commit()?;
        let editable = list != List::Liked && readers_channel == Some(heading.owner.id.to_i64());
        Ok(Some(ListPage {
            card: heading,
            description,
            videos: window.page(rows),
            editable,
        }))
    }
}

#[cfg(test)]
mod tests {
    use youcraft_api::records::Rating;

    use super::*;
    use crate::testing::{ADA, BOB, NEWER, PRIVATE, PUBLIC, UNLISTED, with_videos};

    const ALL: Window = Window {
        offset: 0,
        limit: 50,
    };

    fn ids(page: &ListPage) -> Vec<VideoId> {
        page.videos.items.iter().map(|card| card.id).collect()
    }

    #[test]
    fn watch_later_is_made_when_first_used_and_kept_in_order() {
        let store = with_videos();
        let lists = store.lists_of(BOB, Some(PUBLIC)).unwrap();
        let kinds: Vec<List> = lists.iter().map(|l| l.list).collect();
        assert_eq!(kinds, [List::WatchLater, List::Liked]);
        for video in [PUBLIC, NEWER, UNLISTED, PUBLIC] {
            assert!(matches!(
                store.add_to_list(BOB, List::WatchLater, video, 10).unwrap(),
                Changed::Done(_)
            ));
        }
        assert_eq!(
            store
                .add_to_list(BOB, List::WatchLater, PRIVATE, 10)
                .unwrap(),
            Changed::Missing,
            "not a video Bob may see"
        );
        let page = store
            .list_page(List::WatchLater, Some(BOB), ALL)
            .unwrap()
            .unwrap();
        assert_eq!(ids(&page), [PUBLIC, NEWER, UNLISTED]);
        assert!(page.editable && page.card.videos == 3);
        store
            .move_in_list(BOB, List::WatchLater, UNLISTED, 0, 11)
            .unwrap();
        store
            .move_in_list(BOB, List::WatchLater, PUBLIC, 99, 11)
            .unwrap();
        let page = store
            .list_page(List::WatchLater, Some(BOB), ALL)
            .unwrap()
            .unwrap();
        assert_eq!(ids(&page), [UNLISTED, NEWER, PUBLIC]);
        store
            .remove_from_list(BOB, List::WatchLater, NEWER, 12)
            .unwrap();
        let lists = store.lists_of(BOB, Some(PUBLIC)).unwrap();
        assert!(lists[0].contains && lists[0].videos == 2);
        assert_eq!(
            store
                .list_page(List::WatchLater, Some(ADA), ALL)
                .unwrap()
                .unwrap()
                .card
                .videos,
            0
        );
        assert!(
            store
                .list_page(List::WatchLater, None, ALL)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_private_playlist_shows_to_its_owner_only() {
        let store = with_videos();
        let id = PlaylistId::from_bits(5);
        let made = store
            .create_playlist(&NewPlaylist {
                id,
                owner: BOB,
                title: "Waves",
                visibility: Visibility::Private,
                video: Some(PUBLIC),
                now: 10,
            })
            .unwrap();
        let Changed::Done(card) = made else {
            panic!("{made:?}")
        };
        assert!(card.contains && card.videos == 1 && card.first == Some(PUBLIC));
        let list = List::Playlist(id);
        assert!(store.list_page(list, Some(ADA), ALL).unwrap().is_none());
        assert!(store.list_page(list, Some(BOB), ALL).unwrap().is_some());
        assert_eq!(
            store.add_to_list(ADA, list, NEWER, 11).unwrap(),
            Changed::Missing
        );
        let public = PlaylistEdit {
            title: None,
            description: Some("All about waves."),
            visibility: Some(Visibility::Public),
        };
        store.update_playlist(id, BOB, &public, 12).unwrap();
        let seen = store.list_page(list, None, ALL).unwrap().unwrap();
        assert_eq!(
            (seen.card.title.as_str(), seen.description.as_str()),
            ("Waves", "All about waves.")
        );
        assert!(!seen.editable);
        let shelf = store
            .channel_playlists(crate::testing::BOB_CHANNEL, ALL)
            .unwrap();
        assert_eq!(shelf.items.len(), 1);
        assert!(!store.delete_playlist(id, ADA).unwrap());
        assert!(store.delete_playlist(id, BOB).unwrap());
    }

    #[test]
    fn liked_videos_follow_ratings() {
        let store = with_videos();
        store.rate(BOB, PUBLIC, Rating::Like, 10).unwrap();
        store.rate(BOB, NEWER, Rating::Like, 11).unwrap();
        store.rate(BOB, UNLISTED, Rating::Dislike, 12).unwrap();
        let page = store
            .list_page(List::Liked, Some(BOB), ALL)
            .unwrap()
            .unwrap();
        assert_eq!(ids(&page), [NEWER, PUBLIC]);
        assert!(!page.editable && page.card.videos == 2);
        assert_eq!(
            store.add_to_list(BOB, List::Liked, UNLISTED, 13).unwrap(),
            Changed::Fixed
        );
    }
}
