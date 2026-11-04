//! Lists of videos: the playlists of channels, each channel's Watch later list, and its liked
//! videos, which the `likes` table already holds. A list shows, and counts, only the videos its
//! reader may see. A reader's own Watch later and liked videos are those of the channel they act
//! as; they change a playlist of any channel they speak as ([`crate::SPEAKS_AS`]).

use rusqlite::{OptionalExtension, Row, Transaction, named_params, params};
use youcraft_api::commands::playlist::{PLAYLIST_LIMIT, PlaylistCard, VIDEO_LIMIT};
use youcraft_api::records::{VideoCard, Visibility};
use youcraft_api::{AccountId, ChannelId, List, PlaylistId, VideoId};

use crate::SPEAKS_AS;

use crate::channels::{self, CARD_WIDTH, channel_card};
use crate::videos::{CARD_COLUMNS, VISIBLE, card};
use crate::{Name, Named, Paged, Reader, Store, StoreError, Window, count};

/// What a row of `playlists` is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// A playlist its channel made.
    Playlist,
    /// The channel's Watch later list.
    WatchLater,
}

impl Name for Kind {
    const WHAT: &'static str = "playlist kind";
    fn from_name(name: &str) -> Option<Kind> {
        match name {
            "playlist" => Some(Kind::Playlist),
            "watch_later" => Some(Kind::WatchLater),
            _ => None,
        }
    }
}

/// The items of the playlist `p`, as `i` with their videos `v`.
const ITEMS: &str =
    "playlist_items i JOIN videos v ON v.id = i.video_id WHERE i.playlist_id = p.id";

/// The columns [`playlist_card`] reads, from `playlists` as `p` joined with its channel `o`, for
/// the reader `:reader`, telling whether it holds the video `:containing`.
fn card_columns() -> String {
    format!(
        "p.id, p.kind, p.title, p.visibility, p.updated_at, {}, \
         (SELECT count(*) FROM {ITEMS} AND {VISIBLE}), \
         (SELECT i.video_id FROM {ITEMS} AND {VISIBLE} ORDER BY i.position, i.added_at LIMIT 1), \
         EXISTS (SELECT 1 FROM playlist_items i WHERE i.playlist_id = p.id AND i.video_id = :containing)",
        channels::card_columns("o")
    )
}

/// The tables [`card_columns`] reads.
const FROM: &str = "playlists p JOIN channels o ON o.id = p.channel_id";

fn playlist_card(row: &Row<'_>) -> rusqlite::Result<PlaylistCard> {
    let list = match row.get::<_, Named<Kind>>(1)?.0 {
        Kind::WatchLater => List::WatchLater,
        Kind::Playlist => List::Playlist(PlaylistId::from_i64(row.get(0)?)),
    };
    Ok(PlaylistCard {
        list,
        title: row.get(2)?,
        visibility: row.get::<_, Named<Visibility>>(3)?.0,
        updated_at: row.get(4)?,
        owner: channel_card(row, 5)?,
        videos: count(row.get(5 + CARD_WIDTH)?),
        first: row
            .get::<_, Option<i64>>(6 + CARD_WIDTH)?
            .map(VideoId::from_i64),
        contains: row.get(7 + CARD_WIDTH)?,
    })
}

/// What changing a list found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Changed {
    /// Done: the list now.
    Done(PlaylistCard),
    /// No such list of the reader's, or no such video they may see; or they act as no channel.
    Missing,
    /// The list holds [`VIDEO_LIMIT`] videos, or the channel [`PLAYLIST_LIMIT`] playlists.
    Full,
    /// Liked videos changes only as its channel rates videos.
    Fixed,
}

/// What a list's page shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListPage {
    /// The list.
    pub card: PlaylistCard,
    /// What its channel says about it.
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

/// The card of the playlist row `id` as `reader` sees it, telling whether it holds `containing`.
fn card_of(
    transaction: &Transaction<'_>,
    id: i64,
    reader: Reader,
    containing: Option<VideoId>,
) -> Result<Option<PlaylistCard>, StoreError> {
    let found = transaction
        .query_row(
            &format!("SELECT {} FROM {FROM} WHERE p.id = :id", card_columns()),
            named_params! {
                ":id": id, ":reader": reader.id(), ":admin": reader.admin,
                ":containing": containing.map(VideoId::to_i64)
            },
            playlist_card,
        )
        .optional()?;
    Ok(found)
}

/// The card of the liked videos of the channel `reader` acts as, telling whether they hold
/// `containing`.
fn liked_card(
    transaction: &Transaction<'_>,
    reader: Reader,
    containing: Option<VideoId>,
) -> Result<Option<PlaylistCard>, StoreError> {
    let liked = format!(
        "likes l JOIN videos v ON v.id = l.video_id WHERE l.channel_id = :actor AND l.value = 1 AND {VISIBLE}"
    );
    let found = transaction
        .query_row(
            &format!(
                "SELECT {}, (SELECT count(*) FROM {liked}), \
                 (SELECT l.video_id FROM {liked} ORDER BY l.created_at DESC, v.id LIMIT 1), \
                 (SELECT max(l.created_at) FROM {liked}), \
                 EXISTS (SELECT 1 FROM likes WHERE channel_id = :actor AND value = 1 AND video_id = :containing) \
                 FROM channels o WHERE o.id = :actor",
                channels::card_columns("o")
            ),
            named_params! {
                ":reader": reader.id(), ":actor": reader.actor(), ":admin": reader.admin,
                ":containing": containing.map(VideoId::to_i64)
            },
            |row| {
                Ok(PlaylistCard {
                    list: List::Liked,
                    title: String::new(),
                    visibility: Visibility::Private,
                    owner: channel_card(row, 0)?,
                    videos: count(row.get(CARD_WIDTH)?),
                    first: row.get::<_, Option<i64>>(CARD_WIDTH + 1)?.map(VideoId::from_i64),
                    updated_at: row.get::<_, Option<i64>>(CARD_WIDTH + 2)?.unwrap_or(0),
                    contains: row.get(CARD_WIDTH + 3)?,
                })
            },
        )
        .optional()?;
    Ok(found)
}

/// The Watch later row of `channel`, made now if `make` and it has none yet.
fn watch_later(
    transaction: &Transaction<'_>,
    channel: ChannelId,
    now: i64,
    make: bool,
) -> Result<Option<i64>, StoreError> {
    let find = |transaction: &Transaction<'_>| {
        transaction
            .query_row(
                "SELECT id FROM playlists WHERE channel_id = ?1 AND kind = 'watch_later'",
                [channel.to_i64()],
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
        "INSERT INTO playlists (channel_id, kind, title, created_at, updated_at) \
         VALUES (?1, 'watch_later', '', ?2, ?2)",
        params![channel.to_i64(), now],
    )?;
    Ok(find(transaction)?)
}

/// The row of `list` if it is a playlist of a channel `reader` speaks as, or the Watch later list
/// of the channel they act as; Watch later is made now if `make` and it has none yet.
fn owned(
    transaction: &Transaction<'_>,
    reader: Reader,
    list: List,
    now: i64,
    make: bool,
) -> Result<Option<i64>, StoreError> {
    match (list, reader.channel) {
        (List::WatchLater, Some(channel)) => watch_later(transaction, channel, now, make),
        (List::WatchLater, None) | (List::Liked, _) => Ok(None),
        (List::Playlist(id), _) => Ok(transaction
            .query_row(
                &format!(
                    "SELECT id FROM playlists WHERE id = :id AND kind = 'playlist' \
                     AND channel_id IN {SPEAKS_AS}"
                ),
                named_params! {":id": id.to_i64(), ":reader": reader.id()},
                |row| row.get(0),
            )
            .optional()?),
    }
}

/// The empty Watch later list of `channel`, before anything was put in it.
fn empty_watch_later(
    transaction: &Transaction<'_>,
    channel: ChannelId,
) -> Result<Option<PlaylistCard>, StoreError> {
    let found = transaction
        .query_row(
            &format!(
                "SELECT {} FROM channels o WHERE o.id = ?1",
                channels::card_columns("o")
            ),
            [channel.to_i64()],
            |row| channel_card(row, 0),
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

/// Put `video` last in the playlist row `id`, unless it holds it already, if `reader` may see it.
fn append(
    transaction: &Transaction<'_>,
    id: i64,
    reader: Reader,
    video: VideoId,
    now: i64,
) -> Result<Option<Changed>, StoreError> {
    let seen: Option<i64> = transaction
        .query_row(
            &format!(
                "SELECT v.id FROM videos v WHERE v.id = :video AND {VISIBLE}"
            ),
            named_params! {":video": video.to_i64(), ":reader": reader.id(), ":admin": reader.admin},
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
    /// The lists of the channel `reader` acts as: Watch later, liked videos, then its playlists,
    /// the most recently changed first; each telling whether it holds `containing`. Someone who
    /// acts as no channel has none.
    pub fn lists_of(
        &self,
        reader: Reader,
        containing: Option<VideoId>,
    ) -> Result<Vec<PlaylistCard>, StoreError> {
        let Some(channel) = reader.channel else {
            return Ok(Vec::new());
        };
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let mut lists = Vec::new();
        let later = match watch_later(&transaction, channel, 0, false)? {
            Some(id) => card_of(&transaction, id, reader, containing)?,
            None => empty_watch_later(&transaction, channel)?,
        };
        lists.extend(later);
        lists.extend(liked_card(&transaction, reader, containing)?);
        {
            let mut statement = transaction.prepare_cached(&format!(
                "SELECT {} FROM {FROM} WHERE p.channel_id = :actor AND p.kind = 'playlist' \
                 ORDER BY p.updated_at DESC, p.id",
                card_columns()
            ))?;
            let rows = statement.query_map(
                named_params! {
                    ":reader": reader.id(), ":actor": reader.actor(), ":admin": reader.admin,
                    ":containing": containing.map(VideoId::to_i64)
                },
                playlist_card,
            )?;
            for row in rows {
                lists.push(row?);
            }
        }
        transaction.commit()?;
        Ok(lists)
    }

    /// The public playlists of `channel` as `reader` sees them, the most recently changed first.
    pub fn channel_playlists(
        &self,
        channel: ChannelId,
        reader: Reader,
        window: Window,
    ) -> Result<Paged<PlaylistCard>, StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {} FROM {FROM} WHERE o.id = :channel AND p.kind = 'playlist' AND p.visibility = 'public' \
             ORDER BY p.updated_at DESC, p.id LIMIT :limit OFFSET :offset",
            card_columns()
        ))?;
        let rows = statement.query_map(
            named_params! {
                ":channel": channel.to_i64(), ":reader": reader.id(), ":admin": reader.admin,
                ":containing": None::<i64>, ":limit": window.probe(), ":offset": window.skip()
            },
            playlist_card,
        )?;
        Ok(window.page(rows.collect::<Result<_, _>>()?))
    }

    /// Make `playlist` for the channel `owner` acts as.
    pub fn create_playlist(
        &self,
        owner: Reader,
        playlist: &NewPlaylist<'_>,
    ) -> Result<Changed, StoreError> {
        let Some(channel) = owner.channel else {
            return Ok(Changed::Missing);
        };
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let made: i64 = transaction.query_row(
            "SELECT count(*) FROM playlists WHERE channel_id = ?1 AND kind = 'playlist'",
            [channel.to_i64()],
            |row| row.get(0),
        )?;
        if count(made) >= PLAYLIST_LIMIT {
            return Ok(Changed::Full);
        }
        let id = playlist.id.to_i64();
        transaction.execute(
            "INSERT INTO playlists (id, channel_id, kind, title, visibility, created_at, updated_at) \
             VALUES (?1, ?2, 'playlist', ?3, ?4, ?5, ?5)",
            params![
                id,
                channel.to_i64(),
                playlist.title,
                playlist.visibility.as_str(),
                playlist.now
            ],
        )?;
        if let Some(video) = playlist.video
            && let Some(refused) = append(&transaction, id, owner, video, playlist.now)?
        {
            return Ok(refused);
        }
        let made = card_of(&transaction, id, owner, playlist.video)?;
        transaction.commit()?;
        Ok(made.map_or(Changed::Missing, Changed::Done))
    }

    /// Change the playlist `id` of a channel `owner` speaks as.
    pub fn update_playlist(
        &self,
        id: PlaylistId,
        owner: Reader,
        edit: &PlaylistEdit<'_>,
        now: i64,
    ) -> Result<Changed, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let changed = transaction.execute(
            &format!(
                "UPDATE playlists SET title = COALESCE(:title, title), \
                 description = COALESCE(:description, description), \
                 visibility = COALESCE(:visibility, visibility), updated_at = :now \
                 WHERE id = :id AND kind = 'playlist' AND channel_id IN {SPEAKS_AS}"
            ),
            named_params! {
                ":id": id.to_i64(), ":reader": owner.id(), ":title": edit.title,
                ":description": edit.description,
                ":visibility": edit.visibility.map(Visibility::as_str), ":now": now
            },
        )?;
        if changed == 0 {
            return Ok(Changed::Missing);
        }
        let card = card_of(&transaction, id.to_i64(), owner, None)?;
        transaction.commit()?;
        Ok(card.map_or(Changed::Missing, Changed::Done))
    }

    /// Delete the playlist `id` of a channel `account` speaks as; whether it was one.
    pub fn delete_playlist(&self, id: PlaylistId, account: AccountId) -> Result<bool, StoreError> {
        let deleted = self.connection().execute(
            &format!(
                "DELETE FROM playlists WHERE id = :id AND kind = 'playlist' \
                 AND channel_id IN {SPEAKS_AS}"
            ),
            named_params! {":id": id.to_i64(), ":reader": account.to_i64()},
        )?;
        Ok(deleted > 0)
    }

    /// Put `video` last in `list`, a list of `owner`'s (see [`owned`]).
    pub fn add_to_list(
        &self,
        owner: Reader,
        list: List,
        video: VideoId,
        now: i64,
    ) -> Result<Changed, StoreError> {
        if list == List::Liked {
            return Ok(Changed::Fixed);
        }
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let Some(id) = owned(&transaction, owner, list, now, true)? else {
            return Ok(Changed::Missing);
        };
        if let Some(refused) = append(&transaction, id, owner, video, now)? {
            return Ok(refused);
        }
        let card = card_of(&transaction, id, owner, Some(video))?;
        transaction.commit()?;
        Ok(card.map_or(Changed::Missing, Changed::Done))
    }

    /// Take `video` out of `list`, a list of `owner`'s (see [`owned`]).
    pub fn remove_from_list(
        &self,
        owner: Reader,
        list: List,
        video: VideoId,
        now: i64,
    ) -> Result<Changed, StoreError> {
        if list == List::Liked {
            return Ok(Changed::Fixed);
        }
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let Some(id) = owned(&transaction, owner, list, now, false)? else {
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
        let card = card_of(&transaction, id, owner, Some(video))?;
        transaction.commit()?;
        Ok(card.map_or(Changed::Missing, Changed::Done))
    }

    /// Move `video` to `position` (counted from 0, past the end meaning last) in `list`, a list of
    /// `owner`'s (see [`owned`]).
    pub fn move_in_list(
        &self,
        owner: Reader,
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
        let Some(id) = owned(&transaction, owner, list, now, false)? else {
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
        {
            let mut place = transaction.prepare_cached(
                "UPDATE playlist_items SET position = ?3 WHERE playlist_id = ?1 AND video_id = ?2",
            )?;
            for (at, video) in (0i64..).zip(&order) {
                place.execute(params![id, video, at])?;
            }
        }
        transaction.execute(
            "UPDATE playlists SET updated_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        let card = card_of(&transaction, id, owner, None)?;
        transaction.commit()?;
        Ok(card.map_or(Changed::Missing, Changed::Done))
    }

    /// The page of `list` as `reader` sees it, if they may: Watch later and liked videos are
    /// those of the channel the reader acts as; a playlist shows to all unless it is private, and
    /// then to those who speak as its channel.
    pub fn list_page(
        &self,
        list: List,
        reader: Reader,
        window: Window,
    ) -> Result<Option<ListPage>, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let (heading, description, row, editable) = match (list, reader.channel) {
            (List::Liked, Some(_)) => (
                liked_card(&transaction, reader, None)?,
                String::new(),
                None,
                false,
            ),
            (List::WatchLater, Some(channel)) => {
                match watch_later(&transaction, channel, 0, false)? {
                    Some(id) => (
                        card_of(&transaction, id, reader, None)?,
                        String::new(),
                        Some(id),
                        true,
                    ),
                    None => (
                        empty_watch_later(&transaction, channel)?,
                        String::new(),
                        None,
                        true,
                    ),
                }
            }
            (List::Playlist(id), _) => {
                let found: Option<(String, bool, Named<Visibility>)> = transaction
                    .query_row(
                        &format!(
                            "SELECT description, channel_id IN {SPEAKS_AS}, visibility FROM playlists \
                             WHERE id = :id AND kind = 'playlist'"
                        ),
                        named_params! {":id": id.to_i64(), ":reader": reader.id()},
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                match found {
                    Some((description, speaks, Named(visibility)))
                        if visibility != Visibility::Private || speaks =>
                    {
                        (
                            card_of(&transaction, id.to_i64(), reader, None)?,
                            description,
                            Some(id.to_i64()),
                            speaks,
                        )
                    }
                    _ => (None, String::new(), None, false),
                }
            }
            (_, None) => (None, String::new(), None, false),
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
                         WHERE l.channel_id = :key AND l.value = 1 AND {VISIBLE} \
                         ORDER BY l.created_at DESC, v.id LIMIT :limit OFFSET :offset"
                    ),
                    reader.actor(),
                ),
                (_, Some(id)) => (
                    format!(
                        "SELECT {CARD_COLUMNS} FROM playlist_items i JOIN videos v ON v.id = i.video_id \
                         JOIN channels c ON c.id = v.channel_id WHERE i.playlist_id = :key AND {VISIBLE} \
                         ORDER BY i.position, i.added_at LIMIT :limit OFFSET :offset"
                    ),
                    Some(id),
                ),
                (_, None) => (String::new(), None),
            };
            match key {
                Some(key) => {
                    let mut statement = transaction.prepare_cached(&sql)?;
                    statement
                        .query_map(
                            named_params! {
                                ":key": key, ":reader": reader.id(), ":admin": reader.admin,
                                ":limit": window.probe(), ":offset": window.skip()
                            },
                            card,
                        )?
                        .collect::<Result<_, _>>()?
                }
                None => Vec::new(),
            }
        };
        transaction.commit()?;
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
    use crate::studio::VideoEdit;
    use crate::testing::{
        ADA, BOB, BOB_CHANNEL, NEWER, PRIVATE, PUBLIC, UNLISTED, ada, bob, with_videos,
    };

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
        let bob = bob();
        let lists = store.lists_of(bob, Some(PUBLIC)).unwrap();
        let kinds: Vec<List> = lists.iter().map(|l| l.list).collect();
        assert_eq!(kinds, [List::WatchLater, List::Liked]);
        for video in [PUBLIC, NEWER, UNLISTED, PUBLIC] {
            assert!(matches!(
                store.add_to_list(bob, List::WatchLater, video, 10).unwrap(),
                Changed::Done(_)
            ));
        }
        assert_eq!(
            store
                .add_to_list(bob, List::WatchLater, PRIVATE, 10)
                .unwrap(),
            Changed::Missing,
            "not a video Bob may see"
        );
        let page = store
            .list_page(List::WatchLater, bob, ALL)
            .unwrap()
            .unwrap();
        assert_eq!(ids(&page), [PUBLIC, NEWER, UNLISTED]);
        assert!(page.editable && page.card.videos == 3);
        store
            .move_in_list(bob, List::WatchLater, UNLISTED, 0, 11)
            .unwrap();
        store
            .move_in_list(bob, List::WatchLater, PUBLIC, 99, 11)
            .unwrap();
        let page = store
            .list_page(List::WatchLater, bob, ALL)
            .unwrap()
            .unwrap();
        assert_eq!(ids(&page), [UNLISTED, NEWER, PUBLIC]);
        store
            .remove_from_list(bob, List::WatchLater, NEWER, 12)
            .unwrap();
        let lists = store.lists_of(bob, Some(PUBLIC)).unwrap();
        assert!(lists[0].contains && lists[0].videos == 2);
        assert_eq!(
            store
                .list_page(List::WatchLater, ada(), ALL)
                .unwrap()
                .unwrap()
                .card
                .videos,
            0
        );
        assert!(
            store
                .list_page(List::WatchLater, Reader::ANYONE, ALL)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_private_playlist_shows_to_its_owner_only() {
        let store = with_videos();
        let id = PlaylistId::from_bits(5);
        let (ada, bob) = (ada(), bob());
        let made = store
            .create_playlist(
                bob,
                &NewPlaylist {
                    id,
                    title: "Waves",
                    visibility: Visibility::Private,
                    video: Some(PUBLIC),
                    now: 10,
                },
            )
            .unwrap();
        let Changed::Done(card) = made else {
            panic!("{made:?}")
        };
        assert!(card.contains && card.videos == 1 && card.first == Some(PUBLIC));
        let list = List::Playlist(id);
        assert!(store.list_page(list, ada, ALL).unwrap().is_none());
        assert!(store.list_page(list, bob, ALL).unwrap().is_some());
        assert_eq!(
            store.add_to_list(ada, list, NEWER, 11).unwrap(),
            Changed::Missing
        );
        let public = PlaylistEdit {
            title: None,
            description: Some("All about waves."),
            visibility: Some(Visibility::Public),
        };
        store.update_playlist(id, bob, &public, 12).unwrap();
        let seen = store.list_page(list, Reader::ANYONE, ALL).unwrap().unwrap();
        assert_eq!(
            (seen.card.title.as_str(), seen.description.as_str()),
            ("Waves", "All about waves.")
        );
        assert!(!seen.editable);
        let shelf = store
            .channel_playlists(BOB_CHANNEL, Reader::ANYONE, ALL)
            .unwrap();
        assert_eq!(shelf.items.len(), 1);
        assert!(!store.delete_playlist(id, ADA).unwrap());
        assert!(store.delete_playlist(id, BOB).unwrap());
    }

    /// A list counts, and shows first, only the videos its reader may see: a video made private
    /// leaves the lists of others, and stays in those its channel reads.
    #[test]
    fn a_list_counts_what_its_reader_may_see() {
        let store = with_videos();
        let bob = bob();
        let id = PlaylistId::from_bits(5);
        let new = NewPlaylist {
            id,
            title: "Waves",
            visibility: Visibility::Public,
            video: Some(NEWER),
            now: 10,
        };
        store.create_playlist(bob, &new).unwrap();
        store
            .add_to_list(bob, List::Playlist(id), PUBLIC, 11)
            .unwrap();
        store
            .rate(bob, NEWER, youcraft_api::records::Rating::Like, 12)
            .unwrap();
        let private = VideoEdit {
            visibility: Some(Visibility::Private),
            ..VideoEdit::default()
        };
        store.edit_video(NEWER, &private, 13).unwrap();
        let list = List::Playlist(id);
        let cards = store.lists_of(bob, None).unwrap();
        let (liked, mine) = (&cards[1], &cards[2]);
        assert_eq!((liked.videos, liked.first), (0, None));
        assert_eq!((mine.videos, mine.first), (1, Some(PUBLIC)));
        let page = store.list_page(list, bob, ALL).unwrap().unwrap();
        assert_eq!(ids(&page), [PUBLIC]);
        let shelf = store
            .channel_playlists(BOB_CHANNEL, Reader::ANYONE, ALL)
            .unwrap();
        assert_eq!(shelf.items[0].videos, 1);
        let as_ada = store.list_page(list, ada(), ALL).unwrap().unwrap();
        assert_eq!(
            ids(&as_ada),
            [NEWER, PUBLIC],
            "Ada still sees her own video"
        );
        assert_eq!(as_ada.card.videos, 2);
        assert_eq!(
            store
                .remove_from_list(bob, List::WatchLater, PUBLIC, 14)
                .unwrap(),
            Changed::Missing,
            "taking out of a Watch later list never made does not make it"
        );
        let later: i64 = store
            .connection()
            .query_row(
                "SELECT count(*) FROM playlists WHERE kind = 'watch_later'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(later, 0);
    }

    #[test]
    fn liked_videos_follow_ratings() {
        let store = with_videos();
        let bob = bob();
        store.rate(bob, PUBLIC, Rating::Like, 10).unwrap();
        store.rate(bob, NEWER, Rating::Like, 11).unwrap();
        store.rate(bob, UNLISTED, Rating::Dislike, 12).unwrap();
        let page = store.list_page(List::Liked, bob, ALL).unwrap().unwrap();
        assert_eq!(ids(&page), [NEWER, PUBLIC]);
        assert!(!page.editable && page.card.videos == 2);
        assert_eq!(
            store.add_to_list(bob, List::Liked, UNLISTED, 13).unwrap(),
            Changed::Fixed
        );
    }
}
