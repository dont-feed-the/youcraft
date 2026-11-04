//! Invitations: e-mail addresses that become creators or administrators when they sign in.

use rusqlite::{OptionalExtension, params};
use youcraft_api::Grant;
use youcraft_api::commands::invite::{Invitation, Invited};

use crate::accounts::raise_role;
use crate::{Named, Store, StoreError};

impl Store {
    /// Invite `email` as `role` (creator or admin): at once if an account has that verified
    /// address, else when its person first signs in.
    pub fn invite(&self, email: &str, role: Grant, now: i64) -> Result<Invited, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let holder = transaction
            .query_row(
                "SELECT a.id, a.name FROM accounts a \
                 WHERE a.email = ?1 COLLATE NOCASE AND a.email != ''",
                [email],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let invited = match holder {
            Some((account, name)) => {
                let account = youcraft_api::AccountId::from_i64(account);
                raise_role(&transaction, account, role)?;
                Invited::Applied { account, name }
            }
            None => {
                transaction.execute(
                    "INSERT INTO invitations (email, role, created_at) VALUES (?1, ?2, ?3) \
                     ON CONFLICT (email) DO UPDATE SET role = excluded.role, created_at = excluded.created_at",
                    params![email, role.as_str(), now],
                )?;
                Invited::Waiting {
                    invitation: Invitation {
                        email: email.to_owned(),
                        role,
                        created_at: now,
                    },
                }
            }
        };
        transaction.commit()?;
        Ok(invited)
    }

    /// The invitations still waiting, newest first.
    pub fn invitations(&self) -> Result<Vec<Invitation>, StoreError> {
        let connection = self.connection();
        let mut statement = connection
            .prepare("SELECT email, role, created_at FROM invitations ORDER BY created_at DESC")?;
        let rows = statement.query_map([], |row| {
            Ok(Invitation {
                email: row.get(0)?,
                role: row.get::<_, Named<Grant>>(1)?.0,
                created_at: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Withdraw the invitation of `email`; whether there was one.
    pub fn revoke_invitation(&self, email: &str) -> Result<bool, StoreError> {
        let revoked = self
            .connection()
            .execute("DELETE FROM invitations WHERE email = ?1", [email])?;
        Ok(revoked > 0)
    }
}
