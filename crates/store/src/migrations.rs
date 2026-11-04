//! The schema's history: each migration once, in order, counted in `PRAGMA user_version`.
//!
//! A migration is never edited once it has reached `main`; a change is a new file appended to
//! [`MIGRATIONS`]. Each runs in its own immediate transaction with the version bump, which it
//! reads inside that transaction, so a failure leaves the database at the previous version and
//! two processes opening the file at once never run the same migration twice. Foreign keys are
//! off while migrations run, as SQLite asks of a migration that rebuilds a table: with them on,
//! dropping the old table would delete, through `ON DELETE CASCADE`, every row that refers to it.
//! Before each commit, `PRAGMA foreign_key_check` must find no row that refers to nothing.

use rusqlite::{Connection, TransactionBehavior};

use crate::StoreError;

/// Every migration, oldest first.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001-v1.sql"),
    include_str!("migrations/0002-social.sql"),
    include_str!("migrations/0003-indexes-and-hidden-comments.sql"),
    include_str!("migrations/0004-storage.sql"),
    include_str!("migrations/0005-photos.sql"),
];

/// Bring the schema of `connection` up to date, with foreign keys off while it runs; the caller
/// turns them on.
pub fn run(connection: &mut Connection) -> Result<(), StoreError> {
    apply(connection, MIGRATIONS)
}

/// Bring the schema of `connection` up to the last of `migrations`.
fn apply(connection: &mut Connection, migrations: &[&str]) -> Result<(), StoreError> {
    // Outside any transaction, where SQLite would ignore it.
    connection.pragma_update(None, "foreign_keys", "OFF")?;
    let known = i64::try_from(migrations.len()).unwrap_or(i64::MAX);
    // Each pass applies one migration or returns, so one more pass than migrations suffices.
    for _ in 0..=migrations.len() {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let found: i64 = transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if found > known {
            return Err(StoreError::TooNew { found, known });
        }
        let Some(sql) = usize::try_from(found).ok().and_then(|n| migrations.get(n)) else {
            return Ok(());
        };
        transaction.execute_batch(sql)?;
        let dangling: Option<(String, i64)> = {
            let mut check = transaction.prepare("PRAGMA foreign_key_check")?;
            let mut rows = check.query([])?;
            match rows.next()? {
                Some(row) => Some((row.get(0)?, row.get::<_, Option<i64>>(1)?.unwrap_or(0))),
                None => None,
            }
        };
        let version = found.saturating_add(1);
        if let Some((table, rowid)) = dangling {
            return Err(StoreError::Corrupt(format!(
                "migration {version} leaves row {rowid} of {table} referring to nothing"
            )));
        }
        transaction.pragma_update(None, "user_version", version)?;
        transaction.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(connection: &Connection) -> i64 {
        connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap()
    }

    /// A database of the first version, with a comment, a reply and a like, migrated to now: the
    /// search index takes in its videos, counts are as the triggers would have kept them, and no
    /// row is lost on the way.
    #[test]
    fn a_first_version_database_migrates_with_its_rows() {
        let mut connection = Connection::open_in_memory().unwrap();
        apply(&mut connection, &MIGRATIONS[..1]).unwrap();
        connection
            .execute_batch(
                "INSERT INTO accounts (id, google_subject, email, name, role, created_at) \
                 VALUES (1, 'g1', '', 'Ada', 'creator', 0), (2, 'g2', '', 'Bob', 'viewer', 0), \
                 (3, 'g3', '', 'Eve', 'viewer', 0); \
                 UPDATE accounts SET blocked = 1 WHERE id = 3; \
                 INSERT INTO channels (id, account_id, handle, handle_key, name, created_at) \
                 VALUES (10, 1, 'Ada', 'ada', 'Ada Lovelace', 0), (20, 2, 'bob', 'bob', 'Bob', 0), \
                 (30, 3, 'eve', 'eve', 'Eve', 0); \
                 INSERT INTO videos (id, channel_id, title, description, visibility, state, created_at, published_at) \
                 VALUES (100, 10, 'Ondes et lumière', 'About waves.', 'public', 'ready', 0, 0); \
                 INSERT INTO comments (id, video_id, account_id, parent_id, body, created_at) \
                 VALUES (1, 100, 2, NULL, 'Lovely.', 1), (2, 100, 1, 1, 'Thanks!', 2), \
                 (3, 100, 3, NULL, 'Spam.', 3), (4, 100, 2, 3, 'No.', 4); \
                 INSERT INTO likes (account_id, video_id, value, created_at) VALUES (2, 100, 1, 5);",
            )
            .unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        run(&mut connection).unwrap();
        assert_eq!(version(&connection), MIGRATIONS.len() as i64);
        let found: i64 = connection
            .query_row(
                "SELECT rowid FROM video_search WHERE video_search MATCH 'lumiere'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(found, 100, "the search index holds the videos of before");
        let (comments, rows): (i64, i64) = connection
            .query_row(
                "SELECT comment_count, (SELECT count(*) FROM comments) FROM videos WHERE id = 100",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(rows, 4, "every comment kept");
        assert_eq!(
            comments, 2,
            "Bob's comment and Ada's reply; Eve's thread is hidden"
        );
        let replies: i64 = connection
            .query_row("SELECT reply_count FROM comments WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(replies, 1);
        let likes: i64 = connection
            .query_row("SELECT count(*) FROM likes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(likes, 1);
    }

    /// The queries that read by a reference, and the deletes that follow one, find their rows
    /// through an index.
    #[test]
    fn references_are_looked_up_through_indexes() {
        let mut connection = Connection::open_in_memory().unwrap();
        run(&mut connection).unwrap();
        for (query, index) in [
            (
                "SELECT id FROM comments WHERE parent_id = 1 ORDER BY created_at",
                "comments_replies",
            ),
            (
                "SELECT id FROM comments WHERE account_id = 1",
                "comments_of_account",
            ),
            (
                "SELECT 1 FROM comment_likes WHERE comment_id = 1",
                "comment_likes_of_comment",
            ),
            (
                "SELECT 1 FROM blocks WHERE account_id = 1",
                "blocks_of_account",
            ),
            (
                "SELECT 1 FROM playlist_items WHERE video_id = 1",
                "playlist_items_of_video",
            ),
            (
                "SELECT 1 FROM history WHERE video_id = 1",
                "history_of_video",
            ),
            (
                "SELECT progress_permille FROM jobs WHERE video_id = 1 ORDER BY id DESC LIMIT 1",
                "jobs_of_video",
            ),
        ] {
            let plan: Vec<String> = connection
                .prepare(&format!("EXPLAIN QUERY PLAN {query}"))
                .unwrap()
                .query_map([], |row| row.get(3))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            let plan = plan.join("; ");
            assert!(plan.contains(index), "{query}: {plan}");
            assert!(!plan.contains("TEMP B-TREE"), "{query}: {plan}");
        }
    }

    #[test]
    fn migrations_run_once_and_refuse_a_newer_file() {
        let mut connection = Connection::open_in_memory().unwrap();
        run(&mut connection).unwrap();
        run(&mut connection).unwrap();
        assert_eq!(version(&connection), MIGRATIONS.len() as i64);
        connection.pragma_update(None, "user_version", 999).unwrap();
        assert!(matches!(
            run(&mut connection),
            Err(StoreError::TooNew { found: 999, .. })
        ));
    }

    /// A migration that rebuilds a table others refer to keeps the rows that refer to it.
    #[test]
    fn rebuilding_a_table_keeps_what_refers_to_it() {
        let schema = "CREATE TABLE parent (id INTEGER PRIMARY KEY, name TEXT); \
            CREATE TABLE child (parent_id INTEGER NOT NULL REFERENCES parent (id) ON DELETE CASCADE); \
            INSERT INTO parent VALUES (1, 'a'); INSERT INTO child VALUES (1);";
        let rebuild = "CREATE TABLE parent_new (id INTEGER PRIMARY KEY, name TEXT NOT NULL DEFAULT ''); \
            INSERT INTO parent_new SELECT id, name FROM parent; DROP TABLE parent; \
            ALTER TABLE parent_new RENAME TO parent;";
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        apply(&mut connection, &[schema, rebuild]).unwrap();
        let children: i64 = connection
            .query_row("SELECT count(*) FROM child", [], |row| row.get(0))
            .unwrap();
        assert_eq!(children, 1, "dropping the old table deleted nothing");
        assert_eq!(version(&connection), 2);
    }

    #[test]
    fn a_migration_that_leaves_a_dangling_reference_is_rolled_back() {
        let schema = "CREATE TABLE parent (id INTEGER PRIMARY KEY); \
            CREATE TABLE child (parent_id INTEGER NOT NULL REFERENCES parent (id));";
        let dangling = "INSERT INTO child VALUES (7);";
        let mut connection = Connection::open_in_memory().unwrap();
        assert!(matches!(
            apply(&mut connection, &[schema, dangling]),
            Err(StoreError::Corrupt(_))
        ));
        assert_eq!(version(&connection), 1, "the first migration stays");
        let children: i64 = connection
            .query_row("SELECT count(*) FROM child", [], |row| row.get(0))
            .unwrap();
        assert_eq!(children, 0, "the second is undone");
    }
}
