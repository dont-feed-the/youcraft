//! Search: channels on the first page, then the videos, the best match first.

use youcraft_api::ApiError;
use youcraft_api::commands::search::{Found, QUERY_LIMIT, Query, Search};

use crate::dispatch::Handle;
use crate::handlers::feed::PAGE;
use crate::handlers::input::{line, next, window};
use crate::site::{Caller, Site};

/// How many channels the first page shows.
const CHANNELS: u32 = 3;

impl Handle<Search> for Site {
    async fn handle(&self, caller: Caller, query: Query) -> Result<Found, ApiError> {
        let words = line("query", &query.query, QUERY_LIMIT)?;
        let first = query.after.is_none();
        let window = window(query.after.as_deref(), PAGE)?;
        let viewer = caller.account;
        let searched = words.clone();
        let channels = if first {
            self.db(move |store| store.search_channels(&searched, viewer, CHANNELS))
                .await?
        } else {
            Vec::new()
        };
        let found = self
            .db(move |store| store.search_videos(&words, window))
            .await?;
        Ok(Found {
            channels,
            videos: found.items,
            next: next(window, found.more),
        })
    }
}
