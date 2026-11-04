//! Lists of videos: the playlists of the channel the caller acts as, which whoever speaks as
//! a channel changes, its Watch later and its liked videos.

use youcraft_api::commands::playlist::{
    Add, Create, DESCRIPTION_LIMIT, Delete, Edit, Get, Item, Lists, Mine, Move, NewPlaylist, Open,
    PLAYLIST_LIMIT, Placing, PlaylistCard, PlaylistPage, Remove, TITLE_LIMIT, Update, VIDEO_LIMIT,
    Which,
};
use youcraft_api::commands::site::Nothing;
use youcraft_api::{ApiError, List, PlaylistId, Rule};
use youcraft_store::playlists::{self, Changed, PlaylistEdit};

use crate::dispatch::Handle;
use crate::handlers::feed::PAGE;
use crate::handlers::input::{line, next, optional_text, window};
use crate::secrets::random_bits;
use crate::site::{Caller, Site, now};

/// The card a change answers, or why there is none.
fn changed(change: Changed) -> Result<PlaylistCard, ApiError> {
    match change {
        Changed::Done(card) => Ok(card),
        Changed::Missing => Err(ApiError::NotFound),
        Changed::Full => Err(ApiError::TooLarge { limit: VIDEO_LIMIT }),
        Changed::Fixed => Err(ApiError::invalid("list", Rule::NotOffered)),
    }
}

impl Handle<Create> for Site {
    async fn handle(&self, caller: Caller, new: NewPlaylist) -> Result<PlaylistCard, ApiError> {
        caller.acting()?;
        let title = line("title", &new.title, TITLE_LIMIT)?;
        let (owner, id, now) = (caller.reader(), PlaylistId::from_bits(random_bits()), now());
        let made = self
            .db(move |store| {
                let playlist = playlists::NewPlaylist {
                    id,
                    title: &title,
                    visibility: new.visibility,
                    video: new.video,
                    now,
                };
                store.create_playlist(owner, &playlist)
            })
            .await?;
        match made {
            Changed::Full => Err(ApiError::TooLarge {
                limit: PLAYLIST_LIMIT,
            }),
            other => changed(other),
        }
    }
}

impl Handle<Update> for Site {
    async fn handle(&self, caller: Caller, edit: Edit) -> Result<PlaylistCard, ApiError> {
        caller.signed_in()?;
        let owner = caller.reader();
        let title = edit
            .title
            .map(|title| line("title", &title, TITLE_LIMIT))
            .transpose()?;
        let description = edit
            .description
            .map(|description| optional_text("description", &description, DESCRIPTION_LIMIT))
            .transpose()?;
        let (playlist, visibility, now) = (edit.playlist, edit.visibility, now());
        let change = self
            .db(move |store| {
                let edit = PlaylistEdit {
                    title: title.as_deref(),
                    description: description.as_deref(),
                    visibility,
                };
                store.update_playlist(playlist, owner, &edit, now)
            })
            .await?;
        changed(change)
    }
}

impl Handle<Delete> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Nothing, ApiError> {
        let owner = caller.signed_in()?;
        let deleted = self
            .db(move |store| store.delete_playlist(which.playlist, owner))
            .await?;
        deleted.then_some(Nothing {}).ok_or(ApiError::NotFound)
    }
}

impl Handle<Add> for Site {
    async fn handle(&self, caller: Caller, item: Item) -> Result<PlaylistCard, ApiError> {
        caller.signed_in()?;
        let (owner, now) = (caller.reader(), now());
        changed(
            self.db(move |store| store.add_to_list(owner, item.list, item.video, now))
                .await?,
        )
    }
}

impl Handle<Remove> for Site {
    async fn handle(&self, caller: Caller, item: Item) -> Result<PlaylistCard, ApiError> {
        caller.signed_in()?;
        let (owner, now) = (caller.reader(), now());
        changed(
            self.db(move |store| store.remove_from_list(owner, item.list, item.video, now))
                .await?,
        )
    }
}

impl Handle<Move> for Site {
    async fn handle(&self, caller: Caller, placing: Placing) -> Result<Nothing, ApiError> {
        caller.signed_in()?;
        let (owner, now) = (caller.reader(), now());
        let change = self
            .db(move |store| {
                store.move_in_list(owner, placing.list, placing.video, placing.position, now)
            })
            .await?;
        changed(change).map(|_| Nothing {})
    }
}

impl Handle<Lists> for Site {
    async fn handle(&self, caller: Caller, mine: Mine) -> Result<Vec<PlaylistCard>, ApiError> {
        caller.signed_in()?;
        let owner = caller.reader();
        self.db(move |store| store.lists_of(owner, mine.containing))
            .await
    }
}

impl Handle<Get> for Site {
    async fn handle(&self, caller: Caller, open: Open) -> Result<PlaylistPage, ApiError> {
        let window = window(open.after.as_deref(), PAGE)?;
        let (list, reader) = (open.list, caller.reader());
        let page = self
            .db(move |store| store.list_page(list, reader, window))
            .await?;
        let Some(page) = page else {
            return Err(match (list, caller.account) {
                (List::WatchLater | List::Liked, None) => ApiError::SignInRequired,
                _ => ApiError::NotFound,
            });
        };
        Ok(PlaylistPage {
            card: page.card,
            description: page.description,
            next: next(window, page.videos.more),
            videos: page.videos.items,
            editable: page.editable,
        })
    }
}
