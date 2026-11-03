//! YouCraft's database: one SQLite file.
//!
//! [`Store`] opens the file, brings its schema up to date with the migrations of
//! `src/migrations/` (applied in order and counted in `PRAGMA user_version`), and answers the
//! site's queries, one module per area. SQLite never sees an uploaded file, only our statements
//! with bound parameters. Calls are synchronous; the server runs them off its async threads.

pub mod accounts;
pub mod channels;
pub mod comments;
pub mod credentials;
pub mod invitations;
pub mod jobs;
mod migrations;
pub mod playlists;
pub mod search;
pub mod studio;
pub mod uploads;
pub mod videos;
pub mod watching;

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::Connection;

/// Why a query failed.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// SQLite refused or failed.
    #[error("database: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A stored value breaks the schema's meaning (an unknown enum name, say).
    #[error("corrupt row: {0}")]
    Corrupt(String),
    /// The file holds a newer schema than this program knows.
    #[error("the database is at schema {found}, newer than this program's {known}")]
    TooNew {
        /// Its `user_version`.
        found: i64,
        /// The number of migrations this program has.
        known: i64,
    },
}

/// Which rows of a list to read: `limit` of them, after the first `offset`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    /// How many rows to skip.
    pub offset: u32,
    /// How many rows to read.
    pub limit: u32,
}

/// The rows of a [`Window`], and whether more follow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paged<T> {
    /// The rows.
    pub items: Vec<T>,
    /// Whether rows follow the window.
    pub more: bool,
}

impl Window {
    /// The `LIMIT` to query: one row past the window, which tells whether more follow.
    pub(crate) fn probe(self) -> i64 {
        i64::from(self.limit) + 1
    }

    /// The `OFFSET` to query.
    pub(crate) fn skip(self) -> i64 {
        i64::from(self.offset)
    }

    /// The rows read with [`Window::probe`], cut to the window.
    pub(crate) fn page<T>(self, mut items: Vec<T>) -> Paged<T> {
        let limit = usize::try_from(self.limit).unwrap_or(usize::MAX);
        let more = items.len() > limit;
        items.truncate(limit);
        Paged { items, more }
    }
}

/// The database.
pub struct Store {
    connection: Mutex<Connection>,
}

impl Store {
    /// Open (or create) the database at `path` and migrate it.
    pub fn open(path: &Path) -> Result<Store, StoreError> {
        Store::prepare(Connection::open(path)?)
    }

    /// A fresh database in memory, for tests.
    pub fn in_memory() -> Result<Store, StoreError> {
        Store::prepare(Connection::open_in_memory()?)
    }

    fn prepare(connection: Connection) -> Result<Store, StoreError> {
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        migrations::run(&connection)?;
        Ok(Store {
            connection: Mutex::new(connection),
        })
    }

    /// The connection, for one query or transaction; a panic elsewhere never locks it for good.
    fn connection(&self) -> MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! A database with two people and Ada's videos, for the tests of every module.

    use youcraft_api::time::MediaDuration;
    use youcraft_api::{AccountId, ChannelId, Role, VideoId};

    use crate::Store;
    use crate::accounts::NewAccount;
    use youcraft_api::records::Visibility;

    use crate::videos::NewVideo;

    /// Ada, a creator, and her channel `@Ada`.
    pub const ADA: AccountId = AccountId::from_bits(1);
    pub const ADA_CHANNEL: ChannelId = ChannelId::from_bits(2);
    /// Bob, a viewer, and his channel `@bob`.
    pub const BOB: AccountId = AccountId::from_bits(3);
    pub const BOB_CHANNEL: ChannelId = ChannelId::from_bits(4);
    /// Ada's videos: two public, one unlisted, one private.
    pub const PUBLIC: VideoId = VideoId::from_bits(10);
    pub const NEWER: VideoId = VideoId::from_bits(11);
    pub const UNLISTED: VideoId = VideoId::from_bits(12);
    pub const PRIVATE: VideoId = VideoId::from_bits(13);

    /// The database with Ada, Bob and Ada's videos.
    pub fn with_videos() -> Store {
        let store = Store::in_memory().unwrap();
        for (id, channel, subject, name, handle, role) in [
            (ADA, ADA_CHANNEL, "g1", "Ada Lovelace", "Ada", Role::Creator),
            (BOB, BOB_CHANNEL, "g2", "Bob", "bob", Role::Viewer),
        ] {
            store
                .create_account(&NewAccount {
                    id,
                    channel,
                    google_subject: subject,
                    email: "someone@example.com",
                    name,
                    handle,
                    role,
                    now: 100,
                })
                .unwrap();
        }
        for (id, title, visibility, published_at) in [
            (PUBLIC, "Ondes et lumière", Visibility::Public, 200),
            (NEWER, "Standing waves", Visibility::Public, 300),
            (UNLISTED, "A secret recipe", Visibility::Unlisted, 400),
            (PRIVATE, "Draft", Visibility::Private, 500),
        ] {
            store
                .insert_ready_video(&NewVideo {
                    id,
                    channel: ADA_CHANNEL,
                    title,
                    description: "About waves.",
                    visibility,
                    duration: MediaDuration::from_millis(61_000),
                    width: 1920,
                    height: 1080,
                    published_at,
                })
                .unwrap();
        }
        store
    }
}
