//! The schema's history: each migration once, in order, counted in `PRAGMA user_version`.
//!
//! A migration is never edited once it has reached `main`; a change is a new file appended to
//! [`MIGRATIONS`]. Each runs in its own transaction with the version bump, so a failure leaves
//! the database at the previous version.

use rusqlite::Connection;

use crate::StoreError;

/// Every migration, oldest first.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001-v1.sql"),
    include_str!("migrations/0002-social.sql"),
];

/// Bring the schema of `connection` up to date.
pub fn run(connection: &Connection) -> Result<(), StoreError> {
    let known = i64::try_from(MIGRATIONS.len()).unwrap_or(i64::MAX);
    let found: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if found > known {
        return Err(StoreError::TooNew { found, known });
    }
    for (index, sql) in MIGRATIONS
        .iter()
        .enumerate()
        .skip(usize::try_from(found).unwrap_or(0))
    {
        let version = i64::try_from(index + 1).unwrap_or(i64::MAX);
        let transaction = connection.unchecked_transaction()?;
        transaction.execute_batch(sql)?;
        transaction.pragma_update(None, "user_version", version)?;
        transaction.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_run_once_and_refuse_a_newer_file() {
        let connection = Connection::open_in_memory().unwrap();
        run(&connection).unwrap();
        run(&connection).unwrap();
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
        connection.pragma_update(None, "user_version", 999).unwrap();
        assert!(matches!(
            run(&connection),
            Err(StoreError::TooNew { found: 999, .. })
        ));
    }
}
