//! Accounts and their channels.

use rusqlite::{OptionalExtension, params};
use youcraft_api::{AccountId, ChannelId, Role};

use crate::{Store, StoreError};

/// A new account, as its first sign-in creates it, with the channel every account has.
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

impl Store {
    /// Create an account and its channel; [`StoreError::Sqlite`] if the subject or handle is taken.
    pub fn create_account(&self, account: &NewAccount<'_>) -> Result<(), StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO accounts (id, google_subject, email, name, role, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![account.id.to_i64(), account.google_subject, account.email, account.name, account.role.as_str(), account.now],
        )?;
        transaction.execute(
            "INSERT INTO channels (id, account_id, handle, handle_key, name, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![account.channel.to_i64(), account.id.to_i64(), account.handle, handle_key(account.handle), account.name, account.now],
        )?;
        transaction.commit()?;
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
}

/// The form of a handle that two handles share when they differ only in case.
pub fn handle_key(handle: &str) -> String {
    handle.to_lowercase()
}
