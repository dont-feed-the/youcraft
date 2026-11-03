//! The watch history, which only its owner reads and changes.

use youcraft_api::ApiError;
use youcraft_api::commands::feed::Page;
use youcraft_api::commands::history::{Clear, History, List, Pause, Pausing, Remove};
use youcraft_api::commands::site::Nothing;
use youcraft_api::commands::video::Which;

use crate::dispatch::Handle;
use crate::handlers::feed::PAGE;
use crate::handlers::input::{next, window};
use crate::site::{Caller, Site};

impl Handle<List> for Site {
    async fn handle(&self, caller: Caller, page: Page) -> Result<History, ApiError> {
        let account = caller.signed_in()?;
        let window = window(page.after.as_deref(), PAGE)?;
        let (found, paused) = self.db(move |store| store.history(account, window)).await?;
        Ok(History {
            videos: found.items,
            next: next(window, found.more),
            paused,
        })
    }
}

impl Handle<Remove> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Nothing, ApiError> {
        let account = caller.signed_in()?;
        let removed = self
            .db(move |store| store.forget_watched(account, which.video))
            .await?;
        removed.then_some(Nothing {}).ok_or(ApiError::NotFound)
    }
}

impl Handle<Clear> for Site {
    async fn handle(&self, caller: Caller, _: Nothing) -> Result<Nothing, ApiError> {
        let account = caller.signed_in()?;
        self.db(move |store| store.clear_history(account)).await?;
        Ok(Nothing {})
    }
}

impl Handle<Pause> for Site {
    async fn handle(&self, caller: Caller, pausing: Pausing) -> Result<Pausing, ApiError> {
        let account = caller.signed_in()?;
        self.db(move |store| store.pause_history(account, pausing.paused))
            .await?;
        Ok(pausing)
    }
}
