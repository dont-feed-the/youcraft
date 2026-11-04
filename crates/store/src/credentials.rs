//! Credentials: the sessions of signed-in browsers and the personal tokens of programs.
//!
//! Both are known by the SHA-256 of their secret, which only the browser or program holds, so
//! the database alone signs nobody in. Each lookup answers the account's [`Standing`] as of now,
//! with the channel the credential acts as: the one its person chose with it, while they still
//! manage it, else the first they manage. A new session acts as the person's latest one did; a
//! new token, as the credential that made it.

use rusqlite::{OptionalExtension, params};
use youcraft_api::commands::token::Token;
use youcraft_api::{AccountId, ChannelId, TokenId};

use crate::accounts::{Standing, acting, standing, standing_columns};
use crate::channels::{card_columns, channel_card};
use crate::{Store, StoreError};

/// The SHA-256 of a credential's secret.
pub type SecretHash = [u8; 32];

impl Store {
    /// Open a session for `account` until `expires_at`, and forget every expired one.
    pub fn open_session(
        &self,
        hash: &SecretHash,
        account: AccountId,
        now: i64,
        expires_at: i64,
    ) -> Result<(), StoreError> {
        let connection = self.connection();
        connection.execute("DELETE FROM sessions WHERE expires_at <= ?1", [now])?;
        connection.execute(
            "INSERT INTO sessions (secret_hash, account_id, created_at, expires_at, channel_id) \
             VALUES (?1, ?2, ?3, ?4, (SELECT channel_id FROM sessions WHERE account_id = ?2 \
             ORDER BY created_at DESC LIMIT 1))",
            params![hash.as_slice(), account.to_i64(), now, expires_at],
        )?;
        Ok(())
    }

    /// The standing of the account whose unexpired session is `hash`.
    pub fn session(&self, hash: &SecretHash, now: i64) -> Result<Option<Standing>, StoreError> {
        let row = self
            .connection()
            .query_row(
                &format!(
                    "SELECT {} FROM sessions s JOIN accounts a ON a.id = s.account_id \
                     WHERE s.secret_hash = ?1 AND s.expires_at > ?2",
                    standing_columns("s.channel_id")
                ),
                params![hash.as_slice(), now],
                standing,
            )
            .optional()?;
        Ok(row)
    }

    /// End the session `hash`.
    pub fn close_session(&self, hash: &SecretHash) -> Result<(), StoreError> {
        self.connection().execute(
            "DELETE FROM sessions WHERE secret_hash = ?1",
            [hash.as_slice()],
        )?;
        Ok(())
    }

    /// Have the session `hash` act as `channel` from now on.
    pub fn act_as_in_session(
        &self,
        hash: &SecretHash,
        channel: ChannelId,
    ) -> Result<(), StoreError> {
        self.connection().execute(
            "UPDATE sessions SET channel_id = ?2 WHERE secret_hash = ?1",
            params![hash.as_slice(), channel.to_i64()],
        )?;
        Ok(())
    }

    /// Have the token `hash` act as `channel` from now on.
    pub fn act_as_with_token(
        &self,
        hash: &SecretHash,
        channel: ChannelId,
    ) -> Result<(), StoreError> {
        self.connection().execute(
            "UPDATE tokens SET channel_id = ?2 WHERE secret_hash = ?1",
            params![hash.as_slice(), channel.to_i64()],
        )?;
        Ok(())
    }

    /// Make the token `id` of `account`, acting as `channel`, unless it already has `limit`;
    /// whether it was made.
    pub fn create_token(
        &self,
        (id, hash): (TokenId, &SecretHash),
        (account, channel): (AccountId, Option<ChannelId>),
        name: &str,
        now: i64,
        limit: usize,
    ) -> Result<bool, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let held: i64 = transaction.query_row(
            "SELECT count(*) FROM tokens WHERE account_id = ?1",
            [account.to_i64()],
            |row| row.get(0),
        )?;
        if usize::try_from(held).unwrap_or(usize::MAX) >= limit {
            return Ok(false);
        }
        transaction.execute(
            "INSERT INTO tokens (id, secret_hash, account_id, name, created_at, channel_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id.to_i64(),
                hash.as_slice(),
                account.to_i64(),
                name,
                now,
                channel.map(ChannelId::to_i64)
            ],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    /// The standing of the account whose token is `hash`, noting that it was used `now`.
    pub fn token(&self, hash: &SecretHash, now: i64) -> Result<Option<Standing>, StoreError> {
        let connection = self.connection();
        let used = connection.execute(
            "UPDATE tokens SET last_used_at = ?2 WHERE secret_hash = ?1",
            params![hash.as_slice(), now],
        )?;
        if used == 0 {
            return Ok(None);
        }
        let row = connection
            .query_row(
                &format!(
                    "SELECT {} FROM tokens t JOIN accounts a ON a.id = t.account_id \
                     WHERE t.secret_hash = ?1",
                    standing_columns("t.channel_id")
                ),
                [hash.as_slice()],
                standing,
            )
            .optional()?;
        Ok(row)
    }

    /// The tokens of `account`, newest first, each with the channel it acts as.
    pub fn tokens(&self, account: AccountId) -> Result<Vec<Token>, StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare(&format!(
            "SELECT t.id, t.name, t.created_at, t.last_used_at, {} \
             FROM (SELECT t.*, {} AS acting FROM tokens t WHERE t.account_id = ?1) t \
             LEFT JOIN channels c ON c.id = t.acting ORDER BY t.created_at DESC, t.id",
            card_columns("c"),
            acting("t.account_id", "t.channel_id")
        ))?;
        let tokens = statement
            .query_map([account.to_i64()], |row| {
                let acting: Option<i64> = row.get(4)?;
                Ok(Token {
                    id: TokenId::from_i64(row.get(0)?),
                    name: row.get(1)?,
                    created_at: row.get(2)?,
                    last_used_at: row.get(3)?,
                    channel: match acting {
                        Some(_) => Some(channel_card(row, 4)?),
                        None => None,
                    },
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(tokens)
    }

    /// Revoke the token `id` of `account`; whether it had it.
    pub fn revoke_token(&self, account: AccountId, id: TokenId) -> Result<bool, StoreError> {
        let revoked = self.connection().execute(
            "DELETE FROM tokens WHERE id = ?1 AND account_id = ?2",
            params![id.to_i64(), account.to_i64()],
        )?;
        Ok(revoked > 0)
    }
}

#[cfg(test)]
mod tests {
    use youcraft_api::ChannelId;
    use youcraft_api::Grant;

    use super::*;
    use crate::accounts::GooglePerson;

    fn store_with_account() -> (Store, AccountId) {
        let store = Store::in_memory().unwrap();
        let person = GooglePerson {
            subject: "g1",
            verified_email: None,
            name: "Ada",
        };
        let ids = (AccountId::from_bits(1), ChannelId::from_bits(1));
        let mut handles = std::iter::once("ada".to_owned());
        let standing = store
            .sign_in_with_google(&person, ids, &mut handles, 100)
            .unwrap();
        (store, standing.account)
    }

    #[test]
    fn a_session_signs_in_until_it_expires_or_closes() {
        let (store, ada) = store_with_account();
        store.open_session(&[1; 32], ada, 100, 200).unwrap();
        assert_eq!(store.session(&[1; 32], 150).unwrap().unwrap().account, ada);
        assert_eq!(store.session(&[1; 32], 200).unwrap(), None, "expired");
        assert_eq!(store.session(&[2; 32], 150).unwrap(), None, "unknown");
        store.open_session(&[3; 32], ada, 300, 400).unwrap();
        assert_eq!(
            store.session(&[1; 32], 150).unwrap(),
            None,
            "expired ones are forgotten"
        );
        store.close_session(&[3; 32]).unwrap();
        assert_eq!(store.session(&[3; 32], 350).unwrap(), None);
        store.set_blocked(ada, true).unwrap();
        store.open_session(&[4; 32], ada, 300, 400).unwrap();
        let standing = store.session(&[4; 32], 350).unwrap().unwrap();
        assert!(standing.blocked);
        assert_eq!(standing.role, Grant::Viewer);
    }

    #[test]
    fn tokens_are_made_within_the_limit_used_listed_and_revoked() {
        let (store, ada) = store_with_account();
        let (first, second) = (TokenId::from_bits(1), TokenId::from_bits(2));
        assert!(
            store
                .create_token((first, &[1; 32]), (ada, None), "laptop", 100, 2)
                .unwrap()
        );
        assert!(
            store
                .create_token((second, &[2; 32]), (ada, None), "ci", 110, 2)
                .unwrap()
        );
        assert!(
            !store
                .create_token((TokenId::from_bits(3), &[3; 32]), (ada, None), "x", 120, 2)
                .unwrap()
        );
        assert_eq!(store.token(&[1; 32], 130).unwrap().unwrap().account, ada);
        assert_eq!(store.token(&[9; 32], 130).unwrap(), None);
        let listed = store.tokens(ada).unwrap();
        assert_eq!(
            listed.iter().map(|t| t.id).collect::<Vec<_>>(),
            [second, first]
        );
        assert_eq!(listed[1].last_used_at, Some(130));
        assert!(store.revoke_token(ada, first).unwrap());
        assert!(!store.revoke_token(ada, first).unwrap());
        assert!(
            !store.revoke_token(AccountId::from_bits(8), second).unwrap(),
            "not theirs"
        );
        assert_eq!(store.token(&[1; 32], 140).unwrap(), None);
    }
}
