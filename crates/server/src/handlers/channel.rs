//! Commands about channels: their page and shelves, subscriptions, and creators' blocks.

use youcraft_api::ApiError;
use youcraft_api::commands::channel::{
    Barring, Block, ChannelPage, Get, Handle as ByHandle, Playlists, Shelf, Subscribe, Subscribing,
    Subscription, Subscriptions, Videos,
};
use youcraft_api::commands::playlist::PlaylistShelf;
use youcraft_api::commands::site::Nothing;
use youcraft_api::records::{ChannelCard, Feed};
use youcraft_store::channels::Subscribed;

use crate::dispatch::Handle;
use crate::handlers::feed::PAGE;
use crate::handlers::input::{next, window};
use crate::site::{Caller, Site, now};

/// `handle` as people write it, without the `@`.
pub fn bare(handle: &str) -> String {
    handle.trim().trim_start_matches('@').to_owned()
}

impl Handle<Get> for Site {
    async fn handle(&self, caller: Caller, which: ByHandle) -> Result<ChannelPage, ApiError> {
        let (handle, viewer) = (bare(&which.handle), caller.account);
        self.db(move |store| store.channel_page(&handle, viewer))
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
        let account = caller.signed_in()?;
        let now = now();
        let done = self
            .db(move |store| store.subscribe(account, wish.channel, wish.subscribed, now))
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
        let account = caller.signed_in()?;
        self.db(move |store| store.subscriptions(account)).await
    }
}

impl Handle<Block> for Site {
    async fn handle(&self, caller: Caller, barring: Barring) -> Result<Nothing, ApiError> {
        let me = caller.signed_in()?;
        let handle = bare(&barring.handle);
        let account = self
            .db(move |store| store.account_by_handle(&handle))
            .await?
            .ok_or(ApiError::NotFound)?;
        if account == me {
            return Err(ApiError::Conflict);
        }
        let channel = self
            .db(move |store| store.channel_of(me))
            .await?
            .ok_or(ApiError::NotFound)?;
        let now = now();
        self.db(move |store| store.set_channel_block(channel, account, barring.blocked, now))
            .await?;
        Ok(Nothing {})
    }
}
