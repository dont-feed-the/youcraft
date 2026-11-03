//! Accounts and their channels: creating them, signing in with Google, preferences, roles,
//! blocking.

use rusqlite::{OptionalExtension, Row, Transaction, params};
use youcraft_api::commands::account::{Me, Preferences, Theme};
use youcraft_api::records::ChannelCard;
use youcraft_api::{AccountId, ChannelId, Role};

use crate::{Store, StoreError};

/// A new account, with the channel every account has.
pub struct NewAccount<'a> {
    /// Its identifier.
    pub id: AccountId,
    /// The identifier of its channel.
    pub channel: ChannelId,
    /// The `sub` of the person's Google ID token.
    pub google_subject: &'a str,
    /// Their e-mail address.
    pub email: &'a str,
    /// Their name, which becomes the channel's.
    pub name: &'a str,
    /// The channel's handle, without `@`.
    pub handle: &'a str,
    /// What the person is to the site.
    pub role: Role,
    /// Now, in seconds since 1970.
    pub now: i64,
}

/// A person Google vouched for, signing in.
pub struct GooglePerson<'a> {
    /// The `sub` of their ID token: who they are to Google, for good.
    pub subject: &'a str,
    /// Their e-mail address if Google verified it, else `None`.
    pub verified_email: Option<&'a str>,
    /// Their name, for the channel of a new account.
    pub name: &'a str,
}

/// What an account is to the site now: what a request made with its credentials may do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standing {
    /// The account.
    pub account: AccountId,
    /// Its role.
    pub role: Role,
    /// Whether it is blocked.
    pub blocked: bool,
    /// Its preferences.
    pub preferences: Preferences,
}

/// The columns [`standing`] reads, from `accounts` as `a`.
pub(crate) const STANDING: &str = "a.id, a.role, a.blocked, a.language, a.theme";

/// The [`Standing`] of a row that starts with [`STANDING`].
pub(crate) fn standing(row: &Row<'_>) -> rusqlite::Result<Result<Standing, StoreError>> {
    let (role, theme): (String, String) = (row.get(1)?, row.get(4)?);
    let blocked: i64 = row.get(2)?;
    let account = AccountId::from_i64(row.get(0)?);
    let language: Option<String> = row.get(3)?;
    Ok(match (Role::from_name(&role), Theme::from_name(&theme)) {
        (Some(role), Some(theme)) => Ok(Standing {
            account,
            role,
            blocked: blocked != 0,
            preferences: Preferences { language, theme },
        }),
        _ => Err(StoreError::Corrupt(format!(
            "account {account}: role {role:?}, theme {theme:?}"
        ))),
    })
}

impl Store {
    /// Create an account and its channel; [`StoreError::Sqlite`] if the subject or handle is taken.
    pub fn create_account(&self, account: &NewAccount<'_>) -> Result<(), StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        insert_account(&transaction, account)?;
        transaction.commit()?;
        Ok(())
    }

    /// Sign in the person Google vouched for: their account, created on their first sign-in
    /// with `id`, `channel` and the first free handle of `handles`, its role raised by any
    /// invitation of their verified address, which is then spent.
    pub fn sign_in_with_google(
        &self,
        person: &GooglePerson<'_>,
        (id, channel): (AccountId, ChannelId),
        handles: &mut dyn Iterator<Item = String>,
        now: i64,
    ) -> Result<Standing, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let email = person.verified_email.unwrap_or("");
        let existing: Option<i64> = transaction
            .query_row(
                "SELECT id FROM accounts WHERE google_subject = ?1",
                [person.subject],
                |row| row.get(0),
            )
            .optional()?;
        let account = match existing {
            Some(existing) => {
                transaction.execute(
                    "UPDATE accounts SET email = ?2 WHERE id = ?1",
                    params![existing, email],
                )?;
                AccountId::from_i64(existing)
            }
            None => {
                let handle = free_handle(&transaction, handles)?;
                insert_account(
                    &transaction,
                    &NewAccount {
                        id,
                        channel,
                        google_subject: person.subject,
                        email,
                        name: person.name,
                        handle: &handle,
                        role: Role::Viewer,
                        now,
                    },
                )?;
                id
            }
        };
        if let Some(email) = person.verified_email {
            let invited: Option<String> = transaction
                .query_row(
                    "DELETE FROM invitations WHERE email = ?1 RETURNING role",
                    [email],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(role) = invited.as_deref().and_then(Role::from_name) {
                raise_role(&transaction, account, role)?;
            }
        }
        let standing = standing_of(&transaction, account)?;
        transaction.commit()?;
        standing.ok_or_else(|| StoreError::Corrupt(format!("account {account} vanished")))
    }

    /// The signed-in person `account`, as they see themselves.
    pub fn me(&self, account: AccountId) -> Result<Option<Me>, StoreError> {
        let connection = self.connection();
        let row = connection
            .query_row(
                &format!(
                    "SELECT {STANDING}, c.id, c.handle, c.name FROM accounts a \
                     JOIN channels c ON c.account_id = a.id WHERE a.id = ?1"
                ),
                [account.to_i64()],
                |row| Ok((standing(row)?, channel_card(row, 5)?)),
            )
            .optional()?;
        let Some((standing, channel)) = row else {
            return Ok(None);
        };
        let standing = standing?;
        Ok(Some(Me {
            account,
            channel,
            role: standing.role,
            blocked: standing.blocked,
            preferences: standing.preferences,
        }))
    }

    /// Set the preferences of `account`.
    pub fn set_preferences(
        &self,
        account: AccountId,
        preferences: &Preferences,
    ) -> Result<(), StoreError> {
        self.connection().execute(
            "UPDATE accounts SET language = ?2, theme = ?3 WHERE id = ?1",
            params![
                account.to_i64(),
                preferences.language,
                preferences.theme.as_str()
            ],
        )?;
        Ok(())
    }

    /// The channel whose handle is `handle`, in any case.
    pub fn channel_by_handle(&self, handle: &str) -> Result<Option<ChannelId>, StoreError> {
        let id: Option<i64> = self
            .connection()
            .query_row(
                "SELECT id FROM channels WHERE handle_key = ?1",
                [handle_key(handle)],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id.map(ChannelId::from_i64))
    }

    /// The account whose channel's handle is `handle`, in any case.
    pub fn account_by_handle(&self, handle: &str) -> Result<Option<AccountId>, StoreError> {
        let id: Option<i64> = self
            .connection()
            .query_row(
                "SELECT account_id FROM channels WHERE handle_key = ?1",
                [handle_key(handle)],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id.map(AccountId::from_i64))
    }

    /// Give `account` the role `role`; whether it exists.
    pub fn set_role(&self, account: AccountId, role: Role) -> Result<bool, StoreError> {
        let changed = self.connection().execute(
            "UPDATE accounts SET role = ?2 WHERE id = ?1",
            params![account.to_i64(), role.as_str()],
        )?;
        Ok(changed > 0)
    }

    /// Block or unblock `account`; whether it exists.
    pub fn set_blocked(&self, account: AccountId, blocked: bool) -> Result<bool, StoreError> {
        let changed = self.connection().execute(
            "UPDATE accounts SET blocked = ?2 WHERE id = ?1",
            params![account.to_i64(), i64::from(blocked)],
        )?;
        Ok(changed > 0)
    }
}

fn insert_account(
    transaction: &Transaction<'_>,
    account: &NewAccount<'_>,
) -> Result<(), StoreError> {
    transaction.execute(
        "INSERT INTO accounts (id, google_subject, email, name, role, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![account.id.to_i64(), account.google_subject, account.email, account.name, account.role.as_str(), account.now],
    )?;
    transaction.execute(
        "INSERT INTO channels (id, account_id, handle, handle_key, name, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![account.channel.to_i64(), account.id.to_i64(), account.handle, handle_key(account.handle), account.name, account.now],
    )?;
    Ok(())
}

/// The first of `handles` no channel has; [`StoreError::Corrupt`] if every one is taken.
fn free_handle(
    transaction: &Transaction<'_>,
    handles: &mut dyn Iterator<Item = String>,
) -> Result<String, StoreError> {
    for handle in handles {
        let taken: Option<i64> = transaction
            .query_row(
                "SELECT 1 FROM channels WHERE handle_key = ?1",
                [handle_key(&handle)],
                |row| row.get(0),
            )
            .optional()?;
        if taken.is_none() {
            return Ok(handle);
        }
    }
    Err(StoreError::Corrupt(
        "every handle offered is taken".to_owned(),
    ))
}

/// Raise the role of `account` to `role`, if it is lower.
pub(crate) fn raise_role(
    transaction: &Transaction<'_>,
    account: AccountId,
    role: Role,
) -> Result<(), StoreError> {
    let current: String = transaction.query_row(
        "SELECT role FROM accounts WHERE id = ?1",
        [account.to_i64()],
        |row| row.get(0),
    )?;
    if Role::from_name(&current).is_none_or(|current| current < role) {
        transaction.execute(
            "UPDATE accounts SET role = ?2 WHERE id = ?1",
            params![account.to_i64(), role.as_str()],
        )?;
    }
    Ok(())
}

fn standing_of(
    transaction: &Transaction<'_>,
    account: AccountId,
) -> Result<Option<Standing>, StoreError> {
    let row = transaction
        .query_row(
            &format!("SELECT {STANDING} FROM accounts a WHERE a.id = ?1"),
            [account.to_i64()],
            standing,
        )
        .optional()?;
    row.transpose()
}

/// The channel card in the columns `id, handle, name` starting at `first`.
pub(crate) fn channel_card(row: &Row<'_>, first: usize) -> rusqlite::Result<ChannelCard> {
    Ok(ChannelCard {
        id: ChannelId::from_i64(row.get(first)?),
        handle: row.get(first + 1)?,
        name: row.get(first + 2)?,
    })
}

/// The form of a handle that two handles share when they differ only in case.
pub fn handle_key(handle: &str) -> String {
    handle.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person<'a>(subject: &'a str, email: Option<&'a str>) -> GooglePerson<'a> {
        GooglePerson {
            subject,
            verified_email: email,
            name: "Ada",
        }
    }

    fn sign_in(store: &Store, person: &GooglePerson<'_>, bits: u64) -> Standing {
        let ids = (AccountId::from_bits(bits), ChannelId::from_bits(bits));
        let mut handles = ["ada", "ada2"].into_iter().map(str::to_owned);
        store
            .sign_in_with_google(person, ids, &mut handles, 100)
            .unwrap()
    }

    #[test]
    fn the_first_sign_in_makes_the_account_and_later_ones_find_it() {
        let store = Store::in_memory().unwrap();
        let first = sign_in(&store, &person("g1", Some("ada@example.com")), 1);
        assert_eq!(first.account, AccountId::from_bits(1));
        assert_eq!(first.role, Role::Viewer);
        assert_eq!(first.preferences, Preferences::default());
        let again = sign_in(&store, &person("g1", Some("ada@example.com")), 9);
        assert_eq!(again.account, first.account);
        let other = sign_in(&store, &person("g2", None), 2);
        let me = store.me(other.account).unwrap().unwrap();
        assert_eq!(me.channel.handle, "ada2", "the first free handle");
        assert_eq!(me.channel.name, "Ada");
        assert_eq!(store.me(AccountId::from_bits(77)).unwrap(), None);
    }

    #[test]
    fn an_invitation_waits_for_its_verified_address_then_is_spent() {
        let store = Store::in_memory().unwrap();
        let waiting = store.invite("Ada@Example.com", Role::Creator, 50).unwrap();
        assert!(matches!(
            waiting,
            youcraft_api::commands::invite::Invited::Waiting { .. }
        ));
        let unverified = sign_in(&store, &person("g0", None), 3);
        assert_eq!(unverified.role, Role::Viewer);
        let ada = sign_in(&store, &person("g1", Some("ada@example.com")), 1);
        assert_eq!(ada.role, Role::Creator);
        assert!(store.invitations().unwrap().is_empty());
        let applied = store.invite("ada@example.com", Role::Admin, 60).unwrap();
        assert!(matches!(
            applied,
            youcraft_api::commands::invite::Invited::Applied { .. }
        ));
        assert_eq!(store.me(ada.account).unwrap().unwrap().role, Role::Admin);
        store.invite("ada@example.com", Role::Creator, 70).unwrap();
        assert_eq!(
            store.me(ada.account).unwrap().unwrap().role,
            Role::Admin,
            "an invitation never lowers a role"
        );
        store.invite("bob@example.com", Role::Creator, 80).unwrap();
        assert!(store.revoke_invitation("BOB@example.com").unwrap());
        assert!(!store.revoke_invitation("bob@example.com").unwrap());
    }

    #[test]
    fn preferences_roles_and_blocks_are_kept() {
        let store = Store::in_memory().unwrap();
        let ada = sign_in(&store, &person("g1", None), 1).account;
        let preferences = Preferences {
            language: Some("fr".to_owned()),
            theme: Theme::Dark,
        };
        store.set_preferences(ada, &preferences).unwrap();
        assert_eq!(
            store
                .account_by_handle("@ADA".trim_start_matches('@'))
                .unwrap(),
            Some(ada)
        );
        assert!(store.set_role(ada, Role::Creator).unwrap());
        assert!(store.set_blocked(ada, true).unwrap());
        let me = store.me(ada).unwrap().unwrap();
        assert_eq!(me.preferences, preferences);
        assert_eq!(me.role, Role::Creator);
        assert!(me.blocked);
        assert!(!store.set_blocked(AccountId::from_bits(5), true).unwrap());
    }
}
