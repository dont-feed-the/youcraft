//! Searching: public videos by their title, description and channel's name, and channels by
//! their name and handle, in any script.
//!
//! The words of a query must all be found. A word of three characters or more is looked up in
//! the trigram index, which ignores case and diacritics; a shorter one, which no trigram can
//! hold, falls back to `LIKE`, which ignores the case of ASCII letters only.

use rusqlite::types::Value;
use rusqlite::{ToSql, params_from_iter};
use youcraft_api::AccountId;
use youcraft_api::commands::channel::ChannelPage;
use youcraft_api::records::VideoCard;

use crate::channels::{channel_page, page_columns};
use crate::videos::{CARD_COLUMNS, LISTED, card};
use crate::{Paged, Store, StoreError, Window};

/// The most words of a query that count.
const WORD_LIMIT: usize = 8;

/// The words of `query`, at most [`WORD_LIMIT`].
fn words(query: &str) -> Vec<&str> {
    query.split_whitespace().take(WORD_LIMIT).collect()
}

/// `word` as an FTS5 string: in double quotes, its own doubled.
fn fts_string(word: &str) -> String {
    format!("\"{}\"", word.replace('"', "\"\""))
}

/// `word` as a `LIKE` pattern that finds it anywhere, with `\` escaping `%`, `_` and itself.
fn like_pattern(word: &str) -> String {
    let mut pattern = String::from("%");
    for c in word.chars() {
        if matches!(c, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

impl Store {
    /// The public videos that hold every word of `query`, the best match first.
    pub fn search_videos(
        &self,
        query: &str,
        window: Window,
    ) -> Result<Paged<VideoCard>, StoreError> {
        let words = words(query);
        if words.is_empty() {
            return Ok(Paged {
                items: Vec::new(),
                more: false,
            });
        }
        let (long, short): (Vec<&str>, Vec<&str>) = words
            .into_iter()
            .partition(|word| word.chars().count() >= 3);
        let mut values: Vec<Value> = Vec::new();
        let mut conditions = vec![LISTED.to_owned()];
        let from = if long.is_empty() {
            "videos v JOIN channels c ON c.id = v.channel_id"
        } else {
            let expression: Vec<String> = long.iter().map(|word| fts_string(word)).collect();
            values.push(Value::Text(expression.join(" AND ")));
            conditions.push(format!("video_search MATCH ?{}", values.len()));
            "video_search s JOIN videos v ON v.id = s.rowid JOIN channels c ON c.id = v.channel_id"
        };
        for word in short {
            values.push(Value::Text(like_pattern(word)));
            let n = values.len();
            conditions.push(format!(
                "(v.title LIKE ?{n} ESCAPE '\\' OR v.description LIKE ?{n} ESCAPE '\\' OR c.name LIKE ?{n} ESCAPE '\\')"
            ));
        }
        let order = if long.is_empty() {
            "v.view_count DESC, v.id"
        } else {
            "s.rank, v.view_count DESC, v.id"
        };
        values.push(Value::Integer(window.probe()));
        values.push(Value::Integer(window.skip()));
        let (limit, offset) = (values.len() - 1, values.len());
        let sql = format!(
            "SELECT {CARD_COLUMNS} FROM {from} WHERE {} ORDER BY {order} LIMIT ?{limit} OFFSET ?{offset}",
            conditions.join(" AND ")
        );
        let connection = self.connection();
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(
            params_from_iter(values.iter().map(|v| v as &dyn ToSql)),
            card,
        )?;
        Ok(window.page(rows.collect::<Result<_, _>>()?))
    }

    /// At most `limit` channels whose name or handle holds every word of `query`, the most
    /// subscribed first, as `viewer` sees them.
    pub fn search_channels(
        &self,
        query: &str,
        viewer: Option<AccountId>,
        limit: u32,
    ) -> Result<Vec<ChannelPage>, StoreError> {
        let words = words(query);
        if words.is_empty() {
            return Ok(Vec::new());
        }
        let mut values: Vec<(String, Value)> = vec![
            (":limit".to_owned(), Value::Integer(i64::from(limit))),
            (
                ":reader".to_owned(),
                viewer.map_or(Value::Null, |account| Value::Integer(account.to_i64())),
            ),
        ];
        let mut conditions = Vec::new();
        for (n, word) in words.into_iter().enumerate() {
            let name = format!(":word{n}");
            conditions.push(format!(
                "(c.name LIKE {name} ESCAPE '\\' OR c.handle LIKE {name} ESCAPE '\\')"
            ));
            values.push((name, Value::Text(like_pattern(word))));
        }
        let sql = format!(
            "SELECT {} FROM channels c WHERE {} ORDER BY c.subscriber_count DESC, c.id LIMIT :limit",
            page_columns(),
            conditions.join(" AND ")
        );
        let bound: Vec<(&str, &dyn ToSql)> = values
            .iter()
            .map(|(name, value)| (name.as_str(), value as &dyn ToSql))
            .collect();
        let connection = self.connection();
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(bound.as_slice(), channel_page)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{NEWER, PUBLIC, with_videos};

    const ALL: Window = Window {
        offset: 0,
        limit: 10,
    };

    fn found(store: &Store, query: &str) -> Vec<youcraft_api::VideoId> {
        store
            .search_videos(query, ALL)
            .unwrap()
            .items
            .iter()
            .map(|card| card.id)
            .collect()
    }

    #[test]
    fn every_word_must_be_found_in_any_case_and_accent() {
        let store = with_videos();
        assert_eq!(found(&store, "LUMIERE"), [PUBLIC]);
        assert_eq!(found(&store, "ondes lum"), [PUBLIC]);
        assert_eq!(
            found(&store, "waves"),
            [NEWER, PUBLIC],
            "titles before descriptions"
        );
        assert_eq!(
            found(&store, "et"),
            [PUBLIC],
            "a short word falls back to LIKE"
        );
        assert!(
            found(&store, "recipe").is_empty(),
            "unlisted videos stay out"
        );
        assert!(found(&store, "  ").is_empty());
        assert!(found(&store, "\" OR \"").is_empty(), "quotes are only text");
        assert!(found(&store, "100%").is_empty());
        assert_eq!(
            found(&store, "lovelace").len(),
            2,
            "the channel's name counts"
        );
    }

    #[test]
    fn the_index_holds_every_script() {
        let store = Store::in_memory().unwrap();
        let connection = store.connection();
        connection
            .execute("INSERT INTO video_search (rowid, title, description, channel) VALUES (1, 'Ondes et lumière', '', 'Ada')", [])
            .unwrap();
        connection
            .execute("INSERT INTO video_search (rowid, title, description, channel) VALUES (2, '光の波について', '', 'Bob')", [])
            .unwrap();
        let find = |query: &str| -> Vec<i64> {
            let mut statement = connection
                .prepare("SELECT rowid FROM video_search WHERE video_search MATCH ?1")
                .unwrap();
            statement
                .query_map([query], |row| row.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect()
        };
        assert_eq!(find("lumiere"), [1], "diacritics fold");
        assert_eq!(find("の波に"), [2]);
    }

    #[test]
    fn renaming_a_channel_moves_its_videos_in_the_index() {
        let store = with_videos();
        store
            .connection()
            .execute(
                "UPDATE channels SET name = 'Grace Hopper' WHERE handle = 'Ada'",
                [],
            )
            .unwrap();
        assert!(found(&store, "lovelace").is_empty());
        assert_eq!(found(&store, "hopper").len(), 2);
        let channels = store.search_channels("hop", None, 3).unwrap();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].videos, 2);
    }
}
