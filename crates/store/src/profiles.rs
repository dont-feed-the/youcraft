//! How channels show themselves (their name, description and photo), and the administrators'
//! list of accounts, which names each by its person's name and channels, never by its e-mail
//! address.

use rusqlite::{OptionalExtension, params};
use youcraft_api::commands::admin::AccountRow;
use youcraft_api::commands::profile::Profile;
use youcraft_api::{AccountId, ChannelId, Grant};

use crate::accounts::handle_key;
use crate::channels::{card_columns, channel_card};
use crate::members::memberships_of;
use crate::{Named, Paged, Store, StoreError, Window, signed};

/// What changes about a channel; `None` keeps what it has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProfileEdit<'a> {
    /// Its new name.
    pub name: Option<&'a str>,
    /// Its new description.
    pub description: Option<&'a str>,
}

/// The files made of a channel's photo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhotoFiles {
    /// Their version, which their addresses name.
    pub version: String,
    /// What they fill on disk, which the site's storage counts.
    pub bytes: u64,
}

impl Store {
    /// How `channel` shows itself.
    pub fn profile(&self, channel: ChannelId) -> Result<Option<Profile>, StoreError> {
        let connection = self.connection();
        let profile = connection
            .query_row(
                &format!(
                    "SELECT {}, c.description FROM channels c WHERE c.id = ?1",
                    card_columns("c")
                ),
                [channel.to_i64()],
                |row| {
                    Ok(Profile {
                        card: channel_card(row, 0)?,
                        description: row.get(4)?,
                    })
                },
            )
            .optional()?;
        Ok(profile)
    }

    /// Change `channel` as `edit` says; how it shows itself then.
    pub fn edit_profile(
        &self,
        channel: ChannelId,
        edit: &ProfileEdit<'_>,
    ) -> Result<Option<Profile>, StoreError> {
        self.connection().execute(
            "UPDATE channels SET name = COALESCE(?2, name), description = COALESCE(?3, description) \
             WHERE id = ?1",
            params![channel.to_i64(), edit.name, edit.description],
        )?;
        self.profile(channel)
    }

    /// Name `channel` after its handle again.
    pub fn reset_name(&self, channel: ChannelId) -> Result<Option<Profile>, StoreError> {
        self.connection().execute(
            "UPDATE channels SET name = handle WHERE id = ?1",
            [channel.to_i64()],
        )?;
        self.profile(channel)
    }

    /// Give `channel` the photo `photo`, or none; the version it had, whose files are no longer
    /// named by anything.
    pub fn set_photo(
        &self,
        channel: ChannelId,
        photo: Option<&PhotoFiles>,
    ) -> Result<Option<String>, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let old: Option<String> = transaction
            .query_row(
                "SELECT photo FROM channels WHERE id = ?1",
                [channel.to_i64()],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        transaction.execute(
            "UPDATE channels SET photo = ?2, photo_bytes = ?3 WHERE id = ?1",
            params![
                channel.to_i64(),
                photo.map(|photo| photo.version.as_str()),
                photo.map_or(0, |photo| signed(photo.bytes))
            ],
        )?;
        transaction.commit()?;
        Ok(old)
    }

    /// The accounts whose person's name, or the name or handle of one of whose channels, holds
    /// every word of `query`, ignoring case, the newest first.
    pub fn accounts(&self, query: &str, window: Window) -> Result<Paged<AccountRow>, StoreError> {
        let words: Vec<String> = query
            .split_whitespace()
            .take(8)
            .map(|word| format!("%{}%", like_escaped(&handle_key(word))))
            .collect();
        let mut filter = String::from("1");
        for index in 0..words.len() {
            let at = index + 3;
            filter.push_str(&format!(
                " AND (lower(a.name) LIKE ?{at} ESCAPE '\\' OR EXISTS (SELECT 1 FROM members m \
                 JOIN channels c ON c.id = m.channel_id WHERE m.account_id = a.id \
                 AND (lower(c.name) LIKE ?{at} ESCAPE '\\' OR c.handle_key LIKE ?{at} ESCAPE '\\')))"
            ));
        }
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let rows = {
            let mut statement = transaction.prepare(&format!(
                "SELECT a.id, a.name, a.role, a.blocked, a.created_at FROM accounts a WHERE {filter} \
                 ORDER BY a.created_at DESC, a.id DESC LIMIT ?1 OFFSET ?2"
            ))?;
            let mut values: Vec<rusqlite::types::Value> =
                vec![window.probe().into(), window.skip().into()];
            values.extend(words.into_iter().map(rusqlite::types::Value::from));
            let rows = statement.query_map(rusqlite::params_from_iter(values), |row| {
                Ok(AccountRow {
                    account: AccountId::from_i64(row.get(0)?),
                    name: row.get(1)?,
                    channels: Vec::new(),
                    role: row.get::<_, Named<Grant>>(2)?.0,
                    blocked: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let mut page = window.page(rows);
        for row in &mut page.items {
            row.channels = memberships_of(&transaction, row.account)?
                .into_iter()
                .map(|membership| membership.channel)
                .collect();
        }
        transaction.commit()?;
        Ok(page)
    }
}

/// `text` with LIKE's wildcards (and the escape itself) escaped by a backslash.
fn like_escaped(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{ADA, ADA_CHANNEL, with_videos};
    use crate::uploads::Allowance;

    #[test]
    fn a_channel_changes_its_name_and_photo_and_keeps_its_handle() {
        let store = with_videos();
        let edit = ProfileEdit {
            name: Some("Ada K."),
            description: Some("Notes on the engine."),
        };
        let profile = store.edit_profile(ADA_CHANNEL, &edit).unwrap().unwrap();
        assert_eq!(
            (profile.card.name.as_str(), profile.card.handle.as_str()),
            ("Ada K.", "Ada")
        );
        assert_eq!(profile.description, "Notes on the engine.");
        let kept = store
            .edit_profile(ADA_CHANNEL, &ProfileEdit::default())
            .unwrap()
            .unwrap();
        assert_eq!(kept, profile, "what is left out stays");
        let files = |version: &str, bytes| PhotoFiles {
            version: version.to_owned(),
            bytes,
        };
        let room = || {
            let allowance = Allowance {
                storage: 1000,
                ..Allowance::UNBOUNDED
            };
            store.room(&allowance).unwrap()
        };
        assert_eq!(
            store
                .set_photo(ADA_CHANNEL, Some(&files("v1", 300)))
                .unwrap(),
            None
        );
        assert_eq!(
            store
                .set_photo(ADA_CHANNEL, Some(&files("v2", 200)))
                .unwrap()
                .as_deref(),
            Some("v1")
        );
        let card = store.profile(ADA_CHANNEL).unwrap().unwrap().card;
        assert_eq!(card.photo.as_deref(), Some("v2"));
        assert_eq!(room(), 800, "the photo's files fill the site's storage");
        assert_eq!(
            store.set_photo(ADA_CHANNEL, None).unwrap().as_deref(),
            Some("v2")
        );
        assert_eq!(room(), 1000);
        let reset = store.reset_name(ADA_CHANNEL).unwrap().unwrap();
        assert_eq!(reset.card.name, "Ada");
    }

    #[test]
    fn accounts_are_found_by_every_word_of_their_name_or_handle() {
        let store = with_videos();
        let window = Window {
            offset: 0,
            limit: 50,
        };
        let all = store.accounts("", window).unwrap();
        assert_eq!(all.items.len(), 2);
        let found = store.accounts("LOVE ada", window).unwrap();
        assert_eq!(found.items.len(), 1);
        assert_eq!(found.items[0].account, ADA);
        assert_eq!(found.items[0].channels[0].handle, "Ada");
        assert_eq!(found.items[0].role, Grant::Creator);
        assert!(
            store.accounts("100%", window).unwrap().items.is_empty(),
            "% is a letter"
        );
        let one = store
            .accounts(
                "",
                Window {
                    offset: 0,
                    limit: 1,
                },
            )
            .unwrap();
        assert!(one.more);
    }
}
