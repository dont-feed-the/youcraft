//! What administrators do to accounts: roles and blocks, never to themselves.

use youcraft_api::commands::admin::{Block, Blocking, NewRole, SetRole};
use youcraft_api::commands::site::Nothing;
use youcraft_api::{AccountId, ApiError};

use crate::dispatch::Handle;
use crate::site::{Caller, Site};

impl Site {
    /// The account whose channel is `@handle`, unless it is the caller's own.
    async fn other_account(&self, caller: Caller, handle: &str) -> Result<AccountId, ApiError> {
        let handle = handle.trim().trim_start_matches('@').to_owned();
        let account = self
            .db(move |store| store.account_by_handle(&handle))
            .await?
            .ok_or(ApiError::NotFound)?;
        if caller.account == Some(account) {
            return Err(ApiError::Conflict);
        }
        Ok(account)
    }
}

impl Handle<SetRole> for Site {
    async fn handle(&self, caller: Caller, change: NewRole) -> Result<Nothing, ApiError> {
        let account = self.other_account(caller, &change.handle).await?;
        let changed = self
            .db(move |store| store.set_role(account, change.role))
            .await?;
        changed.then_some(Nothing {}).ok_or(ApiError::NotFound)
    }
}

impl Handle<Block> for Site {
    async fn handle(&self, caller: Caller, change: Blocking) -> Result<Nothing, ApiError> {
        let account = self.other_account(caller, &change.handle).await?;
        let changed = self
            .db(move |store| store.set_blocked(account, change.blocked))
            .await?;
        changed.then_some(Nothing {}).ok_or(ApiError::NotFound)
    }
}
