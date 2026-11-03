//! YouCraft's database: one SQLite file.
//!
//! [`Store`] opens the file, brings its schema up to date with the migrations of
//! `src/migrations/` (applied in order and counted in `PRAGMA user_version`), and answers the
//! site's queries, one module per area. SQLite never sees an uploaded file, only our statements
//! with bound parameters. Calls are synchronous; the server runs them off its async threads.

pub mod accounts;
mod migrations;
pub mod videos;

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
