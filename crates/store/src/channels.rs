//! Channels as people meet them: their page, their public videos, subscribing to them, the
//! subscriptions feed, and the accounts a channel keeps from commenting.

use rusqlite::{OptionalExtension, Row, named_params, params};
use youcraft_api::commands::channel::{ChannelPage, Subscription};
use youcraft_api::records::{ChannelCard, VideoCard};
use youcraft_api::{AccountId, ChannelId};

use crate::accounts::handle_key;
use crate::videos::{CARD_COLUMNS, LISTED, card};
use crate::{Paged, Store, StoreError, Window, count};

/// The [`ChannelCard`] of the columns `id, handle, name` of a channel, from the `first`.
pub(crate) fn channel_card(row: &Row<'_>, first: usize) -> rusqlite::Result<ChannelCard> {
    Ok(ChannelCard {
        id: ChannelId::from_i64(row.get(first)?),
        handle: row.get(first + 1)?,
        name: row.get(first + 2)?,
    })
}

/// The columns [`channel_page`] reads, from `channels` as `c`, for the reader `:reader`.
pub(crate) fn page_columns() -> String {
    format!(
        "c.id, c.handle, c.name, c.description, c.subscriber_count, \
         (SELECT count(*) FROM videos v WHERE v.channel_id = c.id AND {LISTED}), \
         EXISTS (SELECT 1 FROM subscriptions s WHERE s.channel_id = c.id AND s.account_id = :reader), \
         c.account_id IS :reader"
    )
}

/// The [`ChannelPage`] of a row that starts with [`page_columns`].
pub(crate) fn channel_page(row: &Row<'_>) -> rusqlite::Result<ChannelPage> {
    Ok(ChannelPage {
        card: channel_card(row, 0)?,
        description: row.get(3)?,
        subscribers: count(row.get(4)?),
        videos: count(row.get(5)?),
        subscribed: row.get(6)?,
        owned: row.get(7)?,
    })
}

/// What subscribing found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subscribed {
    /// Done: the channel's subscribers now.
    Done(Subscription),
    /// No such channel.
    NoChannel,
    /// It is the subscriber's own channel.
    OwnChannel,
}

impl Store {
    /// The page of the channel whose handle is `handle`, as `viewer` sees it.
    pub fn channel_page(
        &self,
        handle: &str,
        viewer: Option<AccountId>,
    ) -> Result<Option<ChannelPage>, StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {} FROM channels c WHERE c.handle_key = :handle",
            page_columns()
        ))?;
        let page = statement
            .query_row(
                named_params! {":handle": handle_key(handle), ":reader": viewer.map(AccountId::to_i64)},
                channel_page,
            )
            .optional()?;
        Ok(page)
    }

    /// The public videos of `channel` that are ready, newest first.
    pub fn public_videos(
        &self,
        channel: ChannelId,
        window: Window,
    ) -> Result<Paged<VideoCard>, StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {CARD_COLUMNS} FROM videos v JOIN channels c ON c.id = v.channel_id \
             WHERE v.channel_id = ?1 AND {LISTED} \
             ORDER BY v.published_at DESC, v.id LIMIT ?2 OFFSET ?3"
        ))?;
        let rows = statement.query_map(
            params![channel.to_i64(), window.probe(), window.skip()],
            card,
        )?;
        Ok(window.page(rows.collect::<Result<_, _>>()?))
    }

    /// Make `account` subscribe to `channel`, or stop.
    pub fn subscribe(
        &self,
        account: AccountId,
        channel: ChannelId,
        subscribed: bool,
        now: i64,
    ) -> Result<Subscribed, StoreError> {
        let mut connection = self.connection();
        let transaction = connection.transaction()?;
        let owner: Option<i64> = transaction
            .query_row(
                "SELECT account_id FROM channels WHERE id = ?1",
                [channel.to_i64()],
                |row| row.get(0),
            )
            .optional()?;
        match owner {
            None => return Ok(Subscribed::NoChannel),
            Some(owner) if owner == account.to_i64() => return Ok(Subscribed::OwnChannel),
            Some(_) => {}
        }
        if subscribed {
            transaction.execute(
                "INSERT OR IGNORE INTO subscriptions (account_id, channel_id, created_at) VALUES (?1, ?2, ?3)",
                params![account.to_i64(), channel.to_i64(), now],
            )?;
        } else {
            transaction.execute(
                "DELETE FROM subscriptions WHERE account_id = ?1 AND channel_id = ?2",
                params![account.to_i64(), channel.to_i64()],
            )?;
        }
        let subscribers: i64 = transaction.query_row(
            "SELECT subscriber_count FROM channels WHERE id = ?1",
            [channel.to_i64()],
            |row| row.get(0),
        )?;
        transaction.commit()?;
        Ok(Subscribed::Done(Subscription {
            subscribers: count(subscribers),
            subscribed,
        }))
    }

    /// The channels `account` subscribes to, by name.
    pub fn subscriptions(&self, account: AccountId) -> Result<Vec<ChannelCard>, StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(
            "SELECT c.id, c.handle, c.name FROM subscriptions s JOIN channels c ON c.id = s.channel_id \
             WHERE s.account_id = ?1 ORDER BY c.name COLLATE NOCASE, c.id",
        )?;
        let rows = statement.query_map([account.to_i64()], |row| channel_card(row, 0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The public videos of the channels `account` subscribes to, newest first.
    pub fn subscription_feed(
        &self,
        account: AccountId,
        window: Window,
    ) -> Result<Paged<VideoCard>, StoreError> {
        let connection = self.connection();
        let mut statement = connection.prepare_cached(&format!(
            "SELECT {CARD_COLUMNS} FROM subscriptions s JOIN videos v ON v.channel_id = s.channel_id \
             JOIN channels c ON c.id = v.channel_id \
             WHERE s.account_id = ?1 AND {LISTED} \
             ORDER BY v.published_at DESC, v.id LIMIT ?2 OFFSET ?3"
        ))?;
        let rows = statement.query_map(
            params![account.to_i64(), window.probe(), window.skip()],
            card,
        )?;
        Ok(window.page(rows.collect::<Result<_, _>>()?))
    }

    /// Keep `account` from commenting on the videos of `channel`, or let it again.
    pub fn set_channel_block(
        &self,
        channel: ChannelId,
        account: AccountId,
        blocked: bool,
        now: i64,
    ) -> Result<(), StoreError> {
        let connection = self.connection();
        if blocked {
            connection.execute(
                "INSERT OR IGNORE INTO blocks (channel_id, account_id, created_at) VALUES (?1, ?2, ?3)",
                params![channel.to_i64(), account.to_i64(), now],
            )?;
        } else {
            connection.execute(
                "DELETE FROM blocks WHERE channel_id = ?1 AND account_id = ?2",
                params![channel.to_i64(), account.to_i64()],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{ADA, ADA_CHANNEL, BOB, BOB_CHANNEL, with_videos};

    #[test]
    fn subscribing_counts_once_and_never_to_oneself() {
        let store = with_videos();
        let subscribe = |subscribed| store.subscribe(BOB, ADA_CHANNEL, subscribed, 5).unwrap();
        let done = |subscribers, subscribed| {
            Subscribed::Done(Subscription {
                subscribers,
                subscribed,
            })
        };
        assert_eq!(subscribe(true), done(1, true));
        assert_eq!(subscribe(true), done(1, true), "twice is once");
        assert_eq!(
            store.subscribe(ADA, ADA_CHANNEL, true, 5).unwrap(),
            Subscribed::OwnChannel
        );
        assert_eq!(
            store
                .subscribe(BOB, ChannelId::from_bits(77), true, 5)
                .unwrap(),
            Subscribed::NoChannel
        );
        let page = store.channel_page("ADA", Some(BOB)).unwrap().unwrap();
        assert_eq!(
            (page.subscribers, page.subscribed, page.owned),
            (1, true, false)
        );
        assert_eq!(page.videos, 2, "public videos only");
        assert_eq!(store.subscriptions(BOB).unwrap()[0].handle, "Ada");
        let feed = store
            .subscription_feed(
                BOB,
                Window {
                    offset: 0,
                    limit: 1,
                },
            )
            .unwrap();
        assert!(feed.more);
        assert_eq!(feed.items.len(), 1);
        assert_eq!(subscribe(false), done(0, false));
        let own = store.channel_page("bob", Some(BOB)).unwrap().unwrap();
        assert!(own.owned && own.card.id == BOB_CHANNEL);
    }
}
