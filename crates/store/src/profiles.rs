//! How channels show themselves (their name, description and photo), and the administrators'
//! list of accounts, which names each by its channel and never by its e-mail address.

use rusqlite::{OptionalExtension, params};
use youcraft_api::commands::admin::AccountRow;
use youcraft_api::commands::profile::Profile;
use youcraft_api::{AccountId, Grant};

use crate::accounts::handle_key;
use crate::channels::{CARD_WIDTH, card_columns, channel_card};
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
    /// How the channel of `account` shows itself.
    pub fn profile(&self, account: AccountId) -> Result<Option<Profile>, StoreError> {
        let connection = self.connection();
        let profile = connection
            .query_row(
                &format!(
                    "SELECT {}, c.description FROM channels c WHERE c.account_id = ?1",
                    card_columns("c")
                ),
                [account.to_i64()],
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

    /// Change the channel of `account` as `edit` says; how it shows itself then.
    pub fn edit_profile(
        &self,
        account: AccountId,
        edit: &ProfileEdit<'_>,
    ) -> Result<Option<Profile>, StoreError> {
        self.connection().execute(
            "UPDATE channels SET name = COALESCE(?2, name), description = COALESCE(?3, description) \
             WHERE account_id = ?1",
            params![account.to_i64(), edit.name, edit.description],
        )?;
        self.profile(account)
    }

    /// Name the channel of `account` after its handle again.
    pub fn reset_name(&self, account: AccountId) -> Result<Option<Profile>, StoreError> {
        self.connection().execute(
            "UPDATE channels SET name = handle WHERE account_id = ?1",
            [account.to_i64()],
        )?;
        self.profile(account)
    }

    /// Give the channel of `account` the photo `photo`, or none; the version it had, whose
    /// files are no longer named by anything.
    pub fn set_photo(
        &self,
        account: AccountId,
        photo: Option<&PhotoFiles>,
    ) -> Result<Option<String>, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let old: Option<String> = transaction
            .query_row(
                "SELECT photo FROM channels WHERE account_id = ?1",
                [account.to_i64()],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        transaction.execute(
            "UPDATE channels SET photo = ?2, photo_bytes = ?3 WHERE account_id = ?1",
            params![
                account.to_i64(),
                photo.map(|photo| photo.version.as_str()),
                photo.map_or(0, |photo| signed(photo.bytes))
            ],
        )?;
        transaction.commit()?;
        Ok(old)
    }

    /// The accounts whose channel's name or handle holds every word of `query`, ignoring case,
    /// the newest first.
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
                " AND (lower(c.name) LIKE ?{at} ESCAPE '\\' OR c.handle_key LIKE ?{at} ESCAPE '\\')"
            ));
        }
        let connection = self.connection();
        let mut statement = connection.prepare(&format!(
            "SELECT {}, a.role, a.blocked, a.created_at FROM accounts a \
             JOIN channels c ON c.account_id = a.id WHERE {filter} \
             ORDER BY a.created_at DESC, a.id DESC LIMIT ?1 OFFSET ?2",
            card_columns("c")
        ))?;
        let mut values: Vec<rusqlite::types::Value> =
            vec![window.probe().into(), window.skip().into()];
        values.extend(words.into_iter().map(rusqlite::types::Value::from));
        let rows = statement.query_map(rusqlite::params_from_iter(values), |row| {
            Ok(AccountRow {
                channel: channel_card(row, 0)?,
                role: row.get::<_, Named<Grant>>(CARD_WIDTH)?.0,
                blocked: row.get(CARD_WIDTH + 1)?,
                created_at: row.get(CARD_WIDTH + 2)?,
            })
        })?;
        Ok(window.page(rows.collect::<Result<_, _>>()?))
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
    use crate::testing::{ADA, BOB, with_videos};
    use crate::uploads::Allowance;

    #[test]
    fn a_channel_changes_its_name_and_photo_and_keeps_its_handle() {
        let store = with_videos();
        let edit = ProfileEdit {
            name: Some("Ada K."),
            description: Some("Notes on the engine."),
        };
        let profile = store.edit_profile(ADA, &edit).unwrap().unwrap();
        assert_eq!(
            (profile.card.name.as_str(), profile.card.handle.as_str()),
            ("Ada K.", "Ada")
        );
        assert_eq!(profile.description, "Notes on the engine.");
        let kept = store
            .edit_profile(ADA, &ProfileEdit::default())
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
        assert_eq!(store.set_photo(ADA, Some(&files("v1", 300))).unwrap(), None);
        assert_eq!(
            store
                .set_photo(ADA, Some(&files("v2", 200)))
                .unwrap()
                .as_deref(),
            Some("v1")
        );
        let card = store.profile(ADA).unwrap().unwrap().card;
        assert_eq!(card.photo.as_deref(), Some("v2"));
        assert_eq!(room(), 800, "the photo's files fill the site's storage");
        assert_eq!(store.set_photo(ADA, None).unwrap().as_deref(), Some("v2"));
        assert_eq!(room(), 1000);
        let reset = store.reset_name(ADA).unwrap().unwrap();
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
        assert_eq!(found.items[0].channel.handle, "Ada");
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
        let _ = BOB;
    }
}
