//! Commands about channels: making and deleting them, their page and shelves, subscriptions,
//! and the blocks a channel keeps on its videos; and the names a new channel gets.
//!
//! Subscribing and blocking are done as the channel the caller acts as.

use youcraft_api::commands::channel::{
    Barring, Block, ChannelPage, Create, Delete, Get, Handle as ByHandle, NAME_LIMIT, NewChannel,
    Playlists, Shelf, Subscribe, Subscribing, Subscription, Subscriptions, Videos, Which,
};
use youcraft_api::commands::playlist::PlaylistShelf;
use youcraft_api::commands::site::Nothing;
use youcraft_api::records::{ChannelCard, Feed};
use youcraft_api::{ApiError, ChannelId, ChannelRole};
use youcraft_store::channels::Subscribed;
use youcraft_store::members::NewChannel as NewRow;

use crate::dispatch::Handle;
use crate::handlers::feed::PAGE;
use crate::handlers::input::{cleaned, line, next, window};
use crate::picture;
use crate::secrets::random_bits;
use crate::site::{Caller, Site, now};

/// `handle` as people write it, without the `@`.
pub fn bare(handle: &str) -> String {
    handle.trim().trim_start_matches('@').to_owned()
}

/// A name as a channel shows it: one line of text, at most [`NAME_LIMIT`] characters.
pub fn display_name(name: &str) -> String {
    cleaned(name, NAME_LIMIT)
}

/// The handles to offer a new channel named `name`, best first: its letters and digits in
/// lower case (3 to 30 of them, `user` when there are too few), then that with four random
/// digits, then random ones.
pub fn handles(name: &str) -> impl Iterator<Item = String> {
    let letters: String = name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .take(26)
        .collect();
    let base = if letters.chars().count() < 3 {
        "user".to_owned()
    } else {
        letters
    };
    let numbered = base.clone();
    std::iter::once(base)
        .chain((0..20).map(move |_| format!("{numbered}{:04}", random_bits() % 10_000)))
        .chain((0..20).map(|_| format!("user{}", random_bits() % 1_000_000_000_000)))
}

impl Site {
    /// The channel whose handle is `handle`, with or without `@`.
    pub async fn channel_named(&self, handle: &str) -> Result<ChannelId, ApiError> {
        let handle = bare(handle);
        self.db(move |store| store.channel_by_handle(&handle))
            .await?
            .ok_or(ApiError::NotFound)
    }
}

impl Handle<Create> for Site {
    async fn handle(&self, caller: Caller, new: NewChannel) -> Result<ChannelCard, ApiError> {
        let owner = caller.signed_in()?;
        let name = line("name", &new.name, NAME_LIMIT)?;
        let (id, now) = (ChannelId::from_bits(random_bits()), now());
        let limit = self.config().channels_per_account;
        let made = self
            .db(move |store| {
                let row = NewRow {
                    id,
                    owner,
                    name: &name,
                    now,
                };
                store.create_channel(&row, &mut handles(&name), limit)
            })
            .await?
            .ok_or(ApiError::TooLarge {
                limit: u64::try_from(limit).unwrap_or(u64::MAX),
            })?;
        // A program's token or a browser acts as what it just made; the operator acts as none.
        if caller.credential != crate::site::Credential::Operator {
            self.act_as(caller, made.id).await?;
        }
        Ok(made)
    }
}

impl Handle<Delete> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Nothing, ApiError> {
        let channel = which.channel;
        self.allow_in_channel(caller, channel, ChannelRole::Owner)
            .await?;
        let photo = self
            .db(move |store| store.profile(channel))
            .await?
            .ok_or(ApiError::NotFound)?
            .card
            .photo;
        let videos = self
            .db(move |store| store.videos_of_channel(channel))
            .await?;
        self.db(move |store| store.delete_channel(channel)).await?;
        for video in videos {
            // Waits for a piece of its upload being written, which would otherwise make its
            // file again once deleted; the row is gone, so no later piece comes.
            let _held = self.lock_upload(video).await?;
            crate::storage::remove_files_of(self.config(), video).await;
        }
        picture::remove_photo(self, channel, photo).await;
        Ok(Nothing {})
    }
}

impl Handle<Get> for Site {
    async fn handle(&self, caller: Caller, which: ByHandle) -> Result<ChannelPage, ApiError> {
        let (handle, reader) = (bare(&which.handle), caller.reader());
        self.db(move |store| store.channel_page(&handle, reader))
            .await?
            .ok_or(ApiError::NotFound)
    }
}

impl Handle<Videos> for Site {
    async fn handle(&self, _: Caller, shelf: Shelf) -> Result<Feed, ApiError> {
        let window = window(shelf.after.as_deref(), PAGE)?;
        let found = self
            .db(move |store| store.public_videos(shelf.channel, window))
            .await?;
        Ok(Feed {
            videos: found.items,
            next: next(window, found.more),
        })
    }
}

impl Handle<Playlists> for Site {
    async fn handle(&self, caller: Caller, shelf: Shelf) -> Result<PlaylistShelf, ApiError> {
        let window = window(shelf.after.as_deref(), PAGE)?;
        let reader = caller.reader();
        let found = self
            .db(move |store| store.channel_playlists(shelf.channel, reader, window))
            .await?;
        Ok(PlaylistShelf {
            playlists: found.items,
            next: next(window, found.more),
        })
    }
}

impl Handle<Subscribe> for Site {
    async fn handle(&self, caller: Caller, wish: Subscribing) -> Result<Subscription, ApiError> {
        let subscriber = caller.acting()?;
        if wish.subscribed {
            caller.may_act()?;
        }
        let now = now();
        let done = self
            .db(move |store| store.subscribe(subscriber, wish.channel, wish.subscribed, now))
            .await?;
        match done {
            Subscribed::Done(subscription) => Ok(subscription),
            Subscribed::NoChannel => Err(ApiError::NotFound),
            Subscribed::OwnChannel => Err(ApiError::Conflict),
        }
    }
}

impl Handle<Subscriptions> for Site {
    async fn handle(&self, caller: Caller, _: Nothing) -> Result<Vec<ChannelCard>, ApiError> {
        let subscriber = caller.acting()?;
        self.db(move |store| store.subscriptions(subscriber)).await
    }
}

impl Handle<Block> for Site {
    async fn handle(&self, caller: Caller, barring: Barring) -> Result<Nothing, ApiError> {
        let channel = caller.acting()?;
        let barred = self.channel_named(&barring.handle).await?;
        if barred == channel {
            return Err(ApiError::Conflict);
        }
        let now = now();
        self.db(move |store| store.set_channel_block(channel, (barred, barring.blocked), now))
            .await?;
        Ok(Nothing {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_come_from_the_name_in_any_script() {
        assert_eq!(handles("Ada Lovelace").next().unwrap(), "adalovelace");
        assert_eq!(handles("Мария Кюри").next().unwrap(), "мариякюри");
        assert_eq!(handles("A.").next().unwrap(), "user");
        let mut offered = handles("Ada");
        assert_eq!(offered.next().unwrap(), "ada");
        assert!(offered.next().unwrap().starts_with("ada"));
        assert_eq!(handles("x".repeat(99).as_str()).next().unwrap().len(), 26);
    }

    #[test]
    fn names_lose_control_characters_and_length() {
        assert_eq!(display_name(" Ada\u{0}\n "), "Ada");
        assert_eq!(display_name("\u{202E}Ada"), "Ada");
        assert_eq!(display_name(&"é".repeat(300)).chars().count(), NAME_LIMIT);
    }
}
