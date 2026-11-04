//! Who runs each channel: making and deleting channels, their members and roles, and the
//! invitations owners send, which their person accepts or declines.
//!
//! The schema keeps every channel with an owner (`last_owner_stays` in migration 0006); each
//! change here checks it first, so that it answers [`Change::LastOwner`] rather than failing.
//! An invitation is for the address Google vouched for when its person last signed in.

use rusqlite::{OptionalExtension, Transaction, params};
use youcraft_api::commands::account::Membership;
use youcraft_api::commands::member::{Invitation, Member, Members, Offer};
use youcraft_api::records::ChannelCard;
use youcraft_api::{AccountId, ChannelId, ChannelRole, VideoId};

use crate::accounts::{free_handle, handle_key};
use crate::channels::{card_columns, channel_card};
use crate::{Named, Store, StoreError};

/// What changing a member found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// Done.
    Done,
    /// They are not a member.
    NotMember,
    /// They are the channel's last owner, whom it keeps.
    LastOwner,
}

/// What inviting found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Invited {
    /// Done: the invitation waits for its answer.
    Done(Invitation),
    /// The person with that address is a member already.
    Member,
    /// The channel waits on as many invitations as it may.
    Full,
}

/// What accepting found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Joined {
    /// Done: what the person is to the channel now.
    Done(Membership),
    /// No invitation of that channel waits for this person.
    Uninvited,
    /// The person belongs to as many channels as they may, or the channel has as many members.
    Full,
}

/// A channel to make.
pub struct NewChannel<'a> {
    /// Its identifier.
    pub id: ChannelId,
    /// The account that owns it.
    pub owner: AccountId,
    /// Its name.
    pub name: &'a str,
    /// Now, in seconds since 1970.
    pub now: i64,
}

/// Insert the channel `id` with no member yet; the caller adds its first owner in the same
/// transaction.
pub(crate) fn insert_channel(
    transaction: &Transaction<'_>,
    id: ChannelId,
    handle: &str,
    name: &str,
    now: i64,
) -> Result<(), StoreError> {
    transaction.execute(
        "INSERT INTO channels (id, handle, handle_key, name, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id.to_i64(), handle, handle_key(handle), name, now],
    )?;
    Ok(())
}

/// The channels `account` belongs to, the one it joined first first.
pub(crate) fn memberships_of(
    transaction: &Transaction<'_>,
    account: AccountId,
) -> Result<Vec<Membership>, StoreError> {
    let mut statement = transaction.prepare_cached(&format!(
        "SELECT {}, m.role FROM members m JOIN channels c ON c.id = m.channel_id \
         WHERE m.account_id = ?1 ORDER BY m.created_at, m.channel_id",
        card_columns("c")
    ))?;
    let rows = statement.query_map([account.to_i64()], |row| {
        Ok(Membership {
            channel: channel_card(row, 0)?,
            role: row.get::<_, Named<ChannelRole>>(4)?.0,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// How many channels `account` belongs to.
fn channels_held(transaction: &Transaction<'_>, account: AccountId) -> Result<usize, StoreError> {
    let held: i64 = transaction.query_row(
        "SELECT count(*) FROM members WHERE account_id = ?1",
        [account.to_i64()],
        |row| row.get(0),
    )?;
    Ok(usize::try_from(held).unwrap_or(usize::MAX))
}

/// The role of `account` in `channel`, if it is a member, read inside `transaction`.
fn role_in(
    transaction: &Transaction<'_>,
    channel: ChannelId,
    account: AccountId,
) -> Result<Option<ChannelRole>, StoreError> {
    let role: Option<Named<ChannelRole>> = transaction
        .query_row(
            "SELECT role FROM members WHERE channel_id = ?1 AND account_id = ?2",
            params![channel.to_i64(), account.to_i64()],
            |row| row.get(0),
        )
        .optional()?;
    Ok(role.map(|Named(role)| role))
}

/// Whether `account` is the last owner of `channel`.
fn last_owner(
    transaction: &Transaction<'_>,
    channel: ChannelId,
    account: AccountId,
) -> Result<bool, StoreError> {
    let others: i64 = transaction.query_row(
        "SELECT count(*) FROM members WHERE channel_id = ?1 AND role = 'owner' AND account_id != ?2",
        params![channel.to_i64(), account.to_i64()],
        |row| row.get(0),
    )?;
    Ok(others == 0)
}

impl Store {
    /// The role of `account` in `channel`, if it is a member.
    pub fn member_role(
        &self,
        channel: ChannelId,
        account: AccountId,
    ) -> Result<Option<ChannelRole>, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let role = role_in(&transaction, channel, account)?;
        transaction.commit()?;
        Ok(role)
    }

    /// The channel of `video`, if there is such a video, and the role of `account` in it.
    pub fn video_member_role(
        &self,
        video: VideoId,
        account: Option<AccountId>,
    ) -> Result<Option<(ChannelId, Option<ChannelRole>)>, StoreError> {
        let found: Option<(i64, Option<Named<ChannelRole>>)> = self
            .connection()
            .query_row(
                "SELECT v.channel_id, m.role FROM videos v \
                 LEFT JOIN members m ON m.channel_id = v.channel_id AND m.account_id IS ?2 \
                 WHERE v.id = ?1",
                params![video.to_i64(), account.map(AccountId::to_i64)],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(found.map(|(channel, role)| (ChannelId::from_i64(channel), role.map(|Named(r)| r))))
    }

    /// Make `new`, owned by its owner, with the first free handle of `handles`; `None` if the
    /// owner belongs to `limit` channels already.
    pub fn create_channel(
        &self,
        new: &NewChannel<'_>,
        handles: &mut dyn Iterator<Item = String>,
        limit: usize,
    ) -> Result<Option<ChannelCard>, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        if channels_held(&transaction, new.owner)? >= limit {
            return Ok(None);
        }
        let handle = free_handle(&transaction, handles)?;
        insert_channel(&transaction, new.id, &handle, new.name, new.now)?;
        transaction.execute(
            "INSERT INTO members (channel_id, account_id, role, created_at) VALUES (?1, ?2, 'owner', ?3)",
            params![new.id.to_i64(), new.owner.to_i64(), new.now],
        )?;
        transaction.commit()?;
        Ok(Some(ChannelCard {
            id: new.id,
            handle,
            name: new.name.to_owned(),
            photo: None,
        }))
    }

    /// The videos of `channel`, whose files go with it.
    pub fn videos_of_channel(&self, channel: ChannelId) -> Result<Vec<VideoId>, StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare("SELECT id FROM videos WHERE channel_id = ?1")?;
        let rows =
            statement.query_map([channel.to_i64()], |row| Ok(VideoId::from_i64(row.get(0)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Delete `channel` with everything that is its own: videos, comments, playlists, members;
    /// whether it was there.
    pub fn delete_channel(&self, channel: ChannelId) -> Result<bool, StoreError> {
        let deleted = self
            .connection()
            .execute("DELETE FROM channels WHERE id = ?1", [channel.to_i64()])?;
        Ok(deleted > 0)
    }

    /// Who runs `channel`, and the invitations it waits on at `now`.
    pub fn members(&self, channel: ChannelId, now: i64) -> Result<Members, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM channel_invitations WHERE channel_id = ?1 AND expires_at <= ?2",
            params![channel.to_i64(), now],
        )?;
        let members = {
            let mut statement = transaction.prepare_cached(
                "SELECT a.id, a.name, a.email, m.role, m.created_at FROM members m \
                 JOIN accounts a ON a.id = m.account_id WHERE m.channel_id = ?1 \
                 ORDER BY m.created_at, a.id",
            )?;
            let rows = statement.query_map([channel.to_i64()], |row| {
                Ok(Member {
                    account: AccountId::from_i64(row.get(0)?),
                    name: row.get(1)?,
                    email: row.get(2)?,
                    role: row.get::<_, Named<ChannelRole>>(3)?.0,
                    since: row.get(4)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let invitations = {
            let mut statement = transaction.prepare_cached(
                "SELECT email, role, expires_at FROM channel_invitations WHERE channel_id = ?1 \
                 ORDER BY created_at DESC, email",
            )?;
            let rows = statement.query_map([channel.to_i64()], |row| {
                Ok(Invitation {
                    email: row.get(0)?,
                    role: row.get::<_, Named<ChannelRole>>(1)?.0,
                    expires_at: row.get(2)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        transaction.commit()?;
        Ok(Members {
            members,
            invitations,
        })
    }

    /// Invite `email` to `channel` as `role` until `expires_at`, unless it waits on `limit`
    /// invitations already; inviting an address again offers the new role for longer.
    pub fn invite_member(
        &self,
        channel: ChannelId,
        (email, role): (&str, ChannelRole),
        (now, expires_at): (i64, i64),
        limit: usize,
    ) -> Result<Invited, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let member: Option<i64> = transaction
            .query_row(
                "SELECT 1 FROM members m JOIN accounts a ON a.id = m.account_id \
                 WHERE m.channel_id = ?1 AND a.email = ?2 COLLATE NOCASE AND a.email != ''",
                params![channel.to_i64(), email],
                |row| row.get(0),
            )
            .optional()?;
        if member.is_some() {
            return Ok(Invited::Member);
        }
        transaction.execute(
            "DELETE FROM channel_invitations WHERE channel_id = ?1 AND expires_at <= ?2",
            params![channel.to_i64(), now],
        )?;
        let waiting: i64 = transaction.query_row(
            "SELECT count(*) FROM channel_invitations WHERE channel_id = ?1 AND email != ?2",
            params![channel.to_i64(), email],
            |row| row.get(0),
        )?;
        if usize::try_from(waiting).unwrap_or(usize::MAX) >= limit {
            return Ok(Invited::Full);
        }
        transaction.execute(
            "INSERT INTO channel_invitations (channel_id, email, role, created_at, expires_at) \
             VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT (channel_id, email) DO UPDATE SET \
             role = excluded.role, created_at = excluded.created_at, expires_at = excluded.expires_at",
            params![channel.to_i64(), email, role.as_str(), now, expires_at],
        )?;
        transaction.commit()?;
        Ok(Invited::Done(Invitation {
            email: email.to_owned(),
            role,
            expires_at,
        }))
    }

    /// Withdraw the invitation of `email` to `channel`; whether there was one.
    pub fn uninvite(&self, channel: ChannelId, email: &str) -> Result<bool, StoreError> {
        let withdrawn = self.connection().execute(
            "DELETE FROM channel_invitations WHERE channel_id = ?1 AND email = ?2",
            params![channel.to_i64(), email],
        )?;
        Ok(withdrawn > 0)
    }

    /// Make `account` a `role` of `channel`, of which it is a member.
    pub fn set_member_role(
        &self,
        channel: ChannelId,
        account: AccountId,
        role: ChannelRole,
    ) -> Result<Change, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let change = match role_in(&transaction, channel, account)? {
            None => Change::NotMember,
            Some(ChannelRole::Owner)
                if role != ChannelRole::Owner && last_owner(&transaction, channel, account)? =>
            {
                Change::LastOwner
            }
            Some(_) => {
                transaction.execute(
                    "UPDATE members SET role = ?3 WHERE channel_id = ?1 AND account_id = ?2",
                    params![channel.to_i64(), account.to_i64(), role.as_str()],
                )?;
                Change::Done
            }
        };
        transaction.commit()?;
        Ok(change)
    }

    /// Take `account` off `channel`.
    pub fn remove_member(
        &self,
        channel: ChannelId,
        account: AccountId,
    ) -> Result<Change, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let change = match role_in(&transaction, channel, account)? {
            None => Change::NotMember,
            Some(ChannelRole::Owner) if last_owner(&transaction, channel, account)? => {
                Change::LastOwner
            }
            Some(_) => {
                transaction.execute(
                    "DELETE FROM members WHERE channel_id = ?1 AND account_id = ?2",
                    params![channel.to_i64(), account.to_i64()],
                )?;
                Change::Done
            }
        };
        transaction.commit()?;
        Ok(change)
    }

    /// The invitations that wait for the answer of `account` at `now`, the newest first: those
    /// of its verified address, from channels it does not belong to.
    pub fn offers(&self, account: AccountId, now: i64) -> Result<Vec<Offer>, StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {}, i.role, i.expires_at FROM accounts a \
             JOIN channel_invitations i ON i.email = a.email \
             JOIN channels c ON c.id = i.channel_id \
             WHERE a.id = ?1 AND a.email != '' AND i.expires_at > ?2 \
             AND NOT EXISTS (SELECT 1 FROM members m WHERE m.channel_id = c.id AND m.account_id = a.id) \
             ORDER BY i.created_at DESC, c.id",
            card_columns("c")
        ))?;
        let rows = statement.query_map(params![account.to_i64(), now], |row| {
            Ok(Offer {
                channel: channel_card(row, 0)?,
                role: row.get::<_, Named<ChannelRole>>(4)?.0,
                expires_at: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Make `account` join `channel` as its invitation offers, which is then spent; unless the
    /// account belongs to `limits.0` channels, or the channel has `limits.1` members.
    pub fn accept(
        &self,
        account: AccountId,
        channel: ChannelId,
        now: i64,
        limits: (usize, usize),
    ) -> Result<Joined, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let offered: Option<(String, Named<ChannelRole>)> = transaction
            .query_row(
                "SELECT i.email, i.role FROM accounts a \
                 JOIN channel_invitations i ON i.email = a.email AND i.channel_id = ?2 \
                 WHERE a.id = ?1 AND a.email != '' AND i.expires_at > ?3",
                params![account.to_i64(), channel.to_i64(), now],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((email, Named(role))) = offered else {
            return Ok(Joined::Uninvited);
        };
        if role_in(&transaction, channel, account)?.is_some() {
            return Ok(Joined::Uninvited);
        }
        let members: i64 = transaction.query_row(
            "SELECT count(*) FROM members WHERE channel_id = ?1",
            [channel.to_i64()],
            |row| row.get(0),
        )?;
        if channels_held(&transaction, account)? >= limits.0
            || usize::try_from(members).unwrap_or(usize::MAX) >= limits.1
        {
            return Ok(Joined::Full);
        }
        transaction.execute(
            "INSERT INTO members (channel_id, account_id, role, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![channel.to_i64(), account.to_i64(), role.as_str(), now],
        )?;
        transaction.execute(
            "DELETE FROM channel_invitations WHERE channel_id = ?1 AND email = ?2",
            params![channel.to_i64(), email],
        )?;
        let card = transaction.query_row(
            &format!(
                "SELECT {} FROM channels c WHERE c.id = ?1",
                card_columns("c")
            ),
            [channel.to_i64()],
            |row| channel_card(row, 0),
        )?;
        transaction.commit()?;
        Ok(Joined::Done(Membership {
            channel: card,
            role,
        }))
    }

    /// Turn down, for `account`, the invitation of `channel`; whether there was one.
    pub fn decline(&self, account: AccountId, channel: ChannelId) -> Result<bool, StoreError> {
        let declined = self.connection().execute(
            "DELETE FROM channel_invitations WHERE channel_id = ?1 AND email = \
             (SELECT email FROM accounts WHERE id = ?2 AND email != '')",
            params![channel.to_i64(), account.to_i64()],
        )?;
        Ok(declined > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{ADA, ADA_CHANNEL, BOB, with_videos};

    const BRAND: ChannelId = ChannelId::from_bits(40);

    fn brand(store: &Store) -> ChannelCard {
        let new = NewChannel {
            id: BRAND,
            owner: ADA,
            name: "Ada's lab",
            now: 100,
        };
        let mut handles = ["Ada", "adaslab"].into_iter().map(str::to_owned);
        store
            .create_channel(&new, &mut handles, 10)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn a_new_channel_takes_a_free_handle_within_the_limit() {
        let store = with_videos();
        let card = brand(&store);
        assert_eq!(card.handle, "adaslab", "@Ada is taken");
        assert_eq!(
            store.member_role(BRAND, ADA).unwrap(),
            Some(ChannelRole::Owner)
        );
        let new = NewChannel {
            id: ChannelId::from_bits(41),
            owner: ADA,
            name: "More",
            now: 100,
        };
        let mut handles = std::iter::once("more".to_owned());
        assert_eq!(store.create_channel(&new, &mut handles, 2).unwrap(), None);
    }

    #[test]
    fn an_invitation_is_answered_by_its_verified_address_only() {
        let store = with_videos();
        brand(&store);
        store
            .connection()
            .execute(
                "UPDATE accounts SET email = 'bob@example.com' WHERE id = ?1",
                [BOB.to_i64()],
            )
            .unwrap();
        let offered = store
            .invite_member(
                BRAND,
                ("Bob@Example.com", ChannelRole::Editor),
                (100, 200),
                50,
            )
            .unwrap();
        assert!(matches!(offered, Invited::Done(_)));
        assert_eq!(store.offers(BOB, 150).unwrap()[0].channel.id, BRAND);
        assert!(store.offers(BOB, 200).unwrap().is_empty(), "it lapsed");
        assert_eq!(store.offers(ADA, 150).unwrap(), []);
        assert_eq!(
            store.accept(ADA, BRAND, 150, (10, 100)).unwrap(),
            Joined::Uninvited,
            "not Ada's address"
        );
        assert_eq!(
            store.accept(BOB, BRAND, 150, (1, 100)).unwrap(),
            Joined::Full
        );
        let Joined::Done(joined) = store.accept(BOB, BRAND, 150, (10, 100)).unwrap() else {
            panic!("Bob joins");
        };
        assert_eq!(joined.role, ChannelRole::Editor);
        assert_eq!(
            store.accept(BOB, BRAND, 150, (10, 100)).unwrap(),
            Joined::Uninvited,
            "spent"
        );
        assert_eq!(
            store
                .invite_member(
                    BRAND,
                    ("bob@example.com", ChannelRole::Owner),
                    (160, 300),
                    50
                )
                .unwrap(),
            Invited::Member
        );
        let members = store.members(BRAND, 170).unwrap();
        let roles: Vec<_> = members
            .members
            .iter()
            .map(|m| (m.account, m.role))
            .collect();
        assert_eq!(
            roles,
            [(ADA, ChannelRole::Owner), (BOB, ChannelRole::Editor)]
        );
        assert!(members.invitations.is_empty());

        store
            .invite_member(
                BRAND,
                ("eve@example.com", ChannelRole::Viewer),
                (170, 400),
                1,
            )
            .unwrap();
        assert_eq!(
            store
                .invite_member(
                    BRAND,
                    ("cy@example.com", ChannelRole::Viewer),
                    (170, 400),
                    1
                )
                .unwrap(),
            Invited::Full
        );
        assert!(store.uninvite(BRAND, "EVE@example.com").unwrap());
        assert!(!store.uninvite(BRAND, "eve@example.com").unwrap());
        assert!(!store.decline(BOB, BRAND).unwrap());
    }

    #[test]
    fn a_channel_keeps_an_owner_until_it_is_deleted() {
        let store = with_videos();
        brand(&store);
        assert_eq!(store.remove_member(BRAND, ADA).unwrap(), Change::LastOwner);
        assert_eq!(
            store
                .set_member_role(BRAND, ADA, ChannelRole::Manager)
                .unwrap(),
            Change::LastOwner
        );
        assert_eq!(store.remove_member(BRAND, BOB).unwrap(), Change::NotMember);
        store
            .connection()
            .execute(
                "INSERT INTO members (channel_id, account_id, role, created_at) VALUES (?1, ?2, 'owner', 0)",
                params![BRAND.to_i64(), BOB.to_i64()],
            )
            .unwrap();
        assert_eq!(
            store
                .set_member_role(BRAND, ADA, ChannelRole::Manager)
                .unwrap(),
            Change::Done
        );
        assert_eq!(store.remove_member(BRAND, BOB).unwrap(), Change::LastOwner);
        let kept = store.connection().execute(
            "DELETE FROM members WHERE channel_id = ?1 AND account_id = ?2",
            params![BRAND.to_i64(), BOB.to_i64()],
        );
        assert!(kept.is_err(), "the schema keeps the last owner too");
        assert_eq!(store.remove_member(BRAND, ADA).unwrap(), Change::Done);
        assert!(store.delete_channel(BRAND).unwrap());
        assert_eq!(store.member_role(BRAND, BOB).unwrap(), None);
        assert!(!store.delete_channel(BRAND).unwrap());
        assert_eq!(
            store
                .video_member_role(crate::testing::PUBLIC, Some(BOB))
                .unwrap(),
            Some((ADA_CHANNEL, None))
        );
    }
}
