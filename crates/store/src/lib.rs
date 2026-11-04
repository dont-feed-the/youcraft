//! YouCraft's database: one SQLite file.
//!
//! [`Store`] opens the file, brings its schema up to date with the migrations of
//! `src/migrations/` (applied in order and counted in `PRAGMA user_version`), and answers the
//! site's queries, one module per area. SQLite never sees an uploaded file, only our statements
//! with bound parameters. Calls are synchronous; the server runs them off its async threads.
//! Every transaction is immediate: it takes the write lock when it begins, so that a transaction
//! that reads and then writes never finds that another process wrote in between.

pub mod accounts;
pub mod channels;
pub mod comments;
pub mod credentials;
pub mod invitations;
pub mod jobs;
pub mod members;
mod migrations;
pub mod playlists;
pub mod profiles;
pub mod search;
pub mod studio;
pub mod uploads;
pub mod videos;
pub mod watching;

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use rusqlite::{Connection, TransactionBehavior};
use youcraft_api::commands::account::Theme;
use youcraft_api::records::{Failure, Visibility};
use youcraft_api::{AccountId, ChannelId, ChannelRole, Grant};

/// Why a query failed.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// SQLite refused or failed.
    #[error("database: {0}")]
    Sqlite(rusqlite::Error),
    /// A stored value breaks the schema's meaning (an unknown enum name, say).
    #[error("corrupt row: {0}")]
    Corrupt(String),
    /// Every handle offered for a new channel is taken.
    #[error("every handle offered is taken")]
    HandlesTaken,
    /// The file holds a newer schema than this program knows.
    #[error("the database is at schema {found}, newer than this program's {known}")]
    TooNew {
        /// Its `user_version`.
        found: i64,
        /// The number of migrations this program has.
        known: i64,
    },
}

impl From<rusqlite::Error> for StoreError {
    /// A column that holds no [`Name`] of its type is a corrupt row; anything else, SQLite's.
    fn from(error: rusqlite::Error) -> StoreError {
        match error {
            rusqlite::Error::FromSqlConversionFailure(column, _, cause)
                if cause.is::<Unnamed>() =>
            {
                StoreError::Corrupt(format!("column {column}: {cause}"))
            }
            error => StoreError::Sqlite(error),
        }
    }
}

/// Who reads, or changes what they may: the person, if signed in, whose memberships let them
/// manage their channels' videos; the channel they act as, whose likes, subscriptions and
/// history are theirs; and whether they run the site, which shows them every video as its own
/// channel sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reader {
    /// Their account, if they are signed in.
    pub account: Option<AccountId>,
    /// The channel they act as, if they run one.
    pub channel: Option<ChannelId>,
    /// Whether they are an administrator.
    pub admin: bool,
}

impl Reader {
    /// Someone signed out.
    pub const ANYONE: Reader = Reader {
        account: None,
        channel: None,
        admin: false,
    };

    /// The person signed in as `account`, acting as `channel`, who does not run the site.
    pub fn of(account: AccountId, channel: ChannelId) -> Reader {
        Reader {
            account: Some(account),
            channel: Some(channel),
            admin: false,
        }
    }

    /// Their account as the database stores it, or `NULL`: the `:reader` of queries.
    pub(crate) fn id(self) -> Option<i64> {
        self.account.map(AccountId::to_i64)
    }

    /// The channel they act as, as the database stores it, or `NULL`: the `:actor` of queries.
    pub(crate) fn actor(self) -> Option<i64> {
        self.channel.map(ChannelId::to_i64)
    }
}

/// The channels the reader `:reader` speaks as, managers and owners: they may do as the channel
/// whatever it may do, and take back whatever it did.
pub(crate) const SPEAKS_AS: &str = "(SELECT channel_id FROM members WHERE account_id = :reader \
    AND role IN ('manager', 'owner'))";

/// A value the database keeps as one of a few names.
pub(crate) trait Name: Sized {
    /// What it is, for the message of a corrupt row.
    const WHAT: &'static str;
    /// The value named `name`, if it is one.
    fn from_name(name: &str) -> Option<Self>;
}

/// A column that holds a [`Name`]: a stored name that is none of its type's fails the query
/// with [`StoreError::Corrupt`].
pub(crate) struct Named<T>(pub T);

impl<T: Name> FromSql for Named<T> {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let name = value.as_str()?;
        T::from_name(name).map(Named).ok_or_else(|| {
            FromSqlError::Other(Box::new(Unnamed {
                what: T::WHAT,
                name: name.to_owned(),
            }))
        })
    }
}

/// A stored name that is none of what its column holds.
#[derive(Debug, thiserror::Error)]
#[error("unknown {what} {name:?}")]
struct Unnamed {
    what: &'static str,
    name: String,
}

impl Name for Grant {
    const WHAT: &'static str = "role";
    fn from_name(name: &str) -> Option<Grant> {
        Grant::from_name(name)
    }
}

impl Name for ChannelRole {
    const WHAT: &'static str = "channel role";
    fn from_name(name: &str) -> Option<ChannelRole> {
        ChannelRole::from_name(name)
    }
}

impl Name for Theme {
    const WHAT: &'static str = "theme";
    fn from_name(name: &str) -> Option<Theme> {
        Theme::from_name(name)
    }
}

impl Name for Visibility {
    const WHAT: &'static str = "visibility";
    fn from_name(name: &str) -> Option<Visibility> {
        Visibility::from_name(name)
    }
}

impl Name for Failure {
    const WHAT: &'static str = "failure";
    fn from_name(name: &str) -> Option<Failure> {
        Failure::from_name(name)
    }
}

/// A stored count as its type: the schema keeps counts at zero or more.
pub(crate) fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

/// A size or count as SQLite keeps integers, which reach `i64::MAX`.
pub(crate) fn signed(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
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

    fn prepare(mut connection: Connection) -> Result<Store, StoreError> {
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.set_transaction_behavior(TransactionBehavior::Immediate);
        migrations::run(&mut connection)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
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
mod tests {
    use super::*;

    /// A transaction takes the write lock as it begins: another connection may not begin to
    /// write until it ends, so neither finds, as it writes, that the other wrote since it read.
    #[test]
    fn a_transaction_holds_the_write_lock_from_its_start() {
        let dir = std::env::temp_dir().join(format!("youcraft-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("immediate.sqlite");
        let first = Store::open(&path).unwrap();
        let second = Connection::open(&path).unwrap();
        second.busy_timeout(std::time::Duration::ZERO).unwrap();
        {
            let mut connection = first.connection();
            let transaction = connection.transaction().unwrap();
            let busy = second.execute_batch("BEGIN IMMEDIATE; COMMIT;");
            assert!(busy.is_err(), "the first transaction holds the lock");
            transaction.commit().unwrap();
        }
        second.execute_batch("BEGIN IMMEDIATE; COMMIT;").unwrap();
        drop((first, second));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! A database with two people and Ada's videos, for the tests of every module.

    use youcraft_api::time::MediaDuration;
    use youcraft_api::{AccountId, ChannelId, Grant, VideoId};

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

    /// The person `account` as they read, acting as their first channel, whose identifier the
    /// tests make one more than their account's.
    pub fn person(account: AccountId) -> crate::Reader {
        crate::Reader::of(account, ChannelId::from_bits(account.bits() + 1))
    }

    /// Ada as she reads, acting as her channel.
    pub fn ada() -> crate::Reader {
        person(ADA)
    }

    /// Bob as he reads, acting as his channel.
    pub fn bob() -> crate::Reader {
        person(BOB)
    }

    /// The database with Ada, Bob and Ada's videos.
    pub fn with_videos() -> Store {
        let store = Store::in_memory().unwrap();
        for (id, channel, subject, name, handle, role) in [
            (
                ADA,
                ADA_CHANNEL,
                "g1",
                "Ada Lovelace",
                "Ada",
                Grant::Creator,
            ),
            (BOB, BOB_CHANNEL, "g2", "Bob", "bob", Grant::Viewer),
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
