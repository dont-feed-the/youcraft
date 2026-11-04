//! Accounts, the people who sign in: creating them with their first channel, signing in with
//! Google, who they are to themselves, preferences, roles, blocking.

use rusqlite::{OptionalExtension, Row, Transaction, params};
use youcraft_api::commands::account::{Me, Preferences, Theme};
use youcraft_api::{AccountId, ChannelId, ChannelRole, Grant};

use crate::channels::{card_columns, channel_card};
use crate::members::{insert_channel, memberships_of};
use crate::{Named, Store, StoreError};

/// A new account, with the first channel it owns.
pub struct NewAccount<'a> {
    /// Its identifier.
    pub id: AccountId,
    /// The identifier of its channel.
    pub channel: ChannelId,
    /// The `sub` of the person's Google ID token.
    pub google_subject: &'a str,
    /// Their e-mail address.
    pub email: &'a str,
    /// Their name, which their first channel takes too.
    pub name: &'a str,
    /// Their first channel's handle, without `@`.
    pub handle: &'a str,
    /// What the person is to the site.
    pub role: Grant,
    /// Now, in seconds since 1970.
    pub now: i64,
}

/// A person Google vouched for, signing in.
pub struct GooglePerson<'a> {
    /// The `sub` of their ID token: who they are to Google, for good.
    pub subject: &'a str,
    /// Their e-mail address if Google verified it, else `None`.
    pub verified_email: Option<&'a str>,
    /// Their name, for a new account and its first channel.
    pub name: &'a str,
}

/// What an account is to the site now: what a request made with its credentials may do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standing {
    /// The account.
    pub account: AccountId,
    /// Its role.
    pub role: Grant,
    /// Whether it is blocked.
    pub blocked: bool,
    /// Its preferences.
    pub preferences: Preferences,
    /// The channel it acts as: the one its credential chose while it still manages it, else the
    /// first it manages; `None` when it manages none.
    pub channel: Option<ChannelId>,
}

/// The columns [`standing`] reads, from `accounts` as `a`, for the credential whose chosen
/// channel is the column `chosen` (`NULL` for none).
pub(crate) fn standing_columns(chosen: &str) -> String {
    format!(
        "a.id, a.role, a.blocked, a.language, a.theme, {}",
        acting("a.id", chosen)
    )
}

/// The channel the account of the column `account` acts as with a credential whose chosen
/// channel is the column `chosen`: that one while the account manages it, else the first it
/// manages, else `NULL`.
pub(crate) fn acting(account: &str, chosen: &str) -> String {
    format!(
        "(SELECT m.channel_id FROM members m WHERE m.account_id = {account} \
          AND m.role IN ('manager', 'owner') \
          ORDER BY m.channel_id IS {chosen} DESC, m.created_at, m.channel_id LIMIT 1)"
    )
}

/// The [`Standing`] of a row that starts with [`standing_columns`].
pub(crate) fn standing(row: &Row<'_>) -> rusqlite::Result<Standing> {
    Ok(Standing {
        account: AccountId::from_i64(row.get(0)?),
        role: row.get::<_, Named<Grant>>(1)?.0,
        blocked: row.get::<_, i64>(2)? != 0,
        preferences: Preferences {
            language: row.get(3)?,
            theme: row.get::<_, Named<Theme>>(4)?.0,
        },
        channel: row.get::<_, Option<i64>>(5)?.map(ChannelId::from_i64),
    })
}

impl Store {
    /// Create an account and its first channel; [`StoreError::Sqlite`] if the subject or handle
    /// is taken.
    pub fn create_account(&self, account: &NewAccount<'_>) -> Result<(), StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        insert_account(&transaction, account)?;
        transaction.commit()?;
        Ok(())
    }

    /// Sign in the person Google vouched for: their account, created on their first sign-in
    /// with `id`, and a first channel `channel`, which they own, with the first free handle of
    /// `handles`; its role raised by any invitation of their verified address, which is then
    /// spent. Invitations to run channels wait for their answer.
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
                        role: Grant::Viewer,
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
            if let Some(role) = invited.as_deref().and_then(Grant::from_name) {
                raise_role(&transaction, account, role)?;
            }
        }
        let standing = standing_of(&transaction, account)?;
        transaction.commit()?;
        standing.ok_or_else(|| StoreError::Corrupt(format!("account {account} vanished")))
    }

    /// The signed-in person `account`, acting as `channel`, as they see themselves at `now`.
    pub fn me(
        &self,
        account: AccountId,
        channel: Option<ChannelId>,
        now: i64,
    ) -> Result<Option<Me>, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let row = transaction
            .query_row(
                &format!(
                    "SELECT {}, a.name, \
                     (SELECT count(*) FROM channel_invitations i \
                      WHERE i.email = a.email AND a.email != '' AND i.expires_at > ?2 \
                      AND NOT EXISTS (SELECT 1 FROM members m WHERE m.channel_id = i.channel_id AND m.account_id = a.id)) \
                     FROM accounts a WHERE a.id = ?1",
                    standing_columns("NULL")
                ),
                params![account.to_i64(), now],
                |row| Ok((standing(row)?, row.get::<_, String>(6)?, row.get::<_, i64>(7)?)),
            )
            .optional()?;
        let Some((standing, name, invitations)) = row else {
            return Ok(None);
        };
        let acting = match channel {
            Some(channel) => transaction
                .query_row(
                    &format!(
                        "SELECT {}, c.history_paused FROM channels c WHERE c.id = ?1",
                        card_columns("c")
                    ),
                    [channel.to_i64()],
                    |row| Ok((channel_card(row, 0)?, row.get::<_, bool>(4)?)),
                )
                .optional()?,
            None => None,
        };
        let channels = memberships_of(&transaction, account)?;
        transaction.commit()?;
        let (channel, history_paused) = match acting {
            Some((card, paused)) => (Some(card), paused),
            None => (None, false),
        };
        Ok(Some(Me {
            account,
            name,
            channel,
            channels,
            invitations: crate::count(invitations),
            role: standing.role,
            blocked: standing.blocked,
            preferences: standing.preferences,
            history_paused,
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

    /// Give `account` the role `role`; whether it exists.
    pub fn set_role(&self, account: AccountId, role: Grant) -> Result<bool, StoreError> {
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
    insert_channel(
        transaction,
        account.channel,
        account.handle,
        account.name,
        account.now,
    )?;
    transaction.execute(
        "INSERT INTO members (channel_id, account_id, role, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![
            account.channel.to_i64(),
            account.id.to_i64(),
            ChannelRole::Owner.as_str(),
            account.now
        ],
    )?;
    Ok(())
}

/// The first of `handles` no channel has; [`StoreError::HandlesTaken`] if every one is taken.
pub(crate) fn free_handle(
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
    Err(StoreError::HandlesTaken)
}

/// Raise the role of `account` to `role`, if it is lower.
pub(crate) fn raise_role(
    transaction: &Transaction<'_>,
    account: AccountId,
    role: Grant,
) -> Result<(), StoreError> {
    let Named(current): Named<Grant> = transaction.query_row(
        "SELECT role FROM accounts WHERE id = ?1",
        [account.to_i64()],
        |row| row.get(0),
    )?;
    if current < role {
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
            &format!(
                "SELECT {} FROM accounts a WHERE a.id = ?1",
                standing_columns("NULL")
            ),
            [account.to_i64()],
            standing,
        )
        .optional()?;
    Ok(row)
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
        assert_eq!(first.role, Grant::Viewer);
        assert_eq!(first.preferences, Preferences::default());
        let again = sign_in(&store, &person("g1", Some("ada@example.com")), 9);
        assert_eq!(again.account, first.account);
        let other = sign_in(&store, &person("g2", None), 2);
        assert_eq!(
            other.channel,
            Some(ChannelId::from_bits(2)),
            "it owns its channel"
        );
        let me = store
            .me(other.account, other.channel, 100)
            .unwrap()
            .unwrap();
        let channel = me.channel.unwrap();
        assert_eq!(channel.handle, "ada2", "the first free handle");
        assert_eq!(channel.name, "Ada");
        assert_eq!(me.channels[0].role, ChannelRole::Owner);
        assert_eq!(store.me(AccountId::from_bits(77), None, 100).unwrap(), None);
    }

    #[test]
    fn an_invitation_waits_for_its_verified_address_then_is_spent() {
        let store = Store::in_memory().unwrap();
        let waiting = store.invite("Ada@Example.com", Grant::Creator, 50).unwrap();
        assert!(matches!(
            waiting,
            youcraft_api::commands::invite::Invited::Waiting { .. }
        ));
        let unverified = sign_in(&store, &person("g0", None), 3);
        assert_eq!(unverified.role, Grant::Viewer);
        let ada = sign_in(&store, &person("g1", Some("ada@example.com")), 1);
        assert_eq!(ada.role, Grant::Creator);
        assert!(store.invitations().unwrap().is_empty());
        let applied = store.invite("ada@example.com", Grant::Admin, 60).unwrap();
        assert!(matches!(
            applied,
            youcraft_api::commands::invite::Invited::Applied { .. }
        ));
        assert_eq!(
            store.me(ada.account, None, 100).unwrap().unwrap().role,
            Grant::Admin
        );
        store.invite("ada@example.com", Grant::Creator, 70).unwrap();
        assert_eq!(
            store.me(ada.account, None, 100).unwrap().unwrap().role,
            Grant::Admin,
            "an invitation never lowers a role"
        );
        store.invite("bob@example.com", Grant::Creator, 80).unwrap();
        assert!(store.revoke_invitation("BOB@example.com").unwrap());
        assert!(!store.revoke_invitation("bob@example.com").unwrap());
    }

    #[test]
    fn handles_are_unique_in_any_case() {
        let store = crate::testing::with_videos();
        assert_eq!(
            store.channel_by_handle("ADA").unwrap(),
            Some(crate::testing::ADA_CHANNEL)
        );
        let taken = store.create_account(&NewAccount {
            id: AccountId::from_bits(7),
            channel: ChannelId::from_bits(8),
            google_subject: "g7",
            email: "c@example.com",
            name: "Cy",
            handle: "aDa",
            role: Grant::Viewer,
            now: 100,
        });
        assert!(taken.is_err());
        let ids = (AccountId::from_bits(9), ChannelId::from_bits(9));
        let mut handles = ["ada", "BOB"].into_iter().map(str::to_owned);
        assert!(matches!(
            store.sign_in_with_google(&person("g9", None), ids, &mut handles, 100),
            Err(StoreError::HandlesTaken)
        ));
    }

    #[test]
    fn a_stored_name_that_means_nothing_is_a_corrupt_row() {
        let store = Store::in_memory().unwrap();
        let ada = sign_in(&store, &person("g1", None), 1).account;
        store
            .connection()
            .execute_batch(
                "PRAGMA ignore_check_constraints = ON; UPDATE accounts SET theme = 'sepia';",
            )
            .unwrap();
        assert!(matches!(
            store.me(ada, None, 100),
            Err(StoreError::Corrupt(_))
        ));
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
        assert!(store.set_role(ada, Grant::Creator).unwrap());
        assert!(store.set_blocked(ada, true).unwrap());
        let me = store.me(ada, None, 100).unwrap().unwrap();
        assert_eq!(me.preferences, preferences);
        assert_eq!(me.role, Grant::Creator);
        assert!(me.blocked);
        assert!(!store.set_blocked(AccountId::from_bits(5), true).unwrap());
    }
}
