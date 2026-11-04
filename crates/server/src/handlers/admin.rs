//! What administrators do to accounts: list them, change roles and block, never to themselves,
//! and take back a channel's name or photo that should not show, their own included.

use youcraft_api::commands::admin::{
    ACCOUNTS_PAGE, Accounts, AccountsPage, Block, Blocking, Finding, NewRole, RemovePhoto,
    ResetName, SetRole,
};
use youcraft_api::commands::channel::Handle as ByHandle;
use youcraft_api::commands::profile::Profile;
use youcraft_api::commands::search::QUERY_LIMIT;
use youcraft_api::commands::site::Nothing;
use youcraft_api::{AccountId, ApiError};

use crate::dispatch::Handle;
use crate::handlers::input::{line, next, window};
use crate::site::{Caller, Site};

/// `account`, unless it is the caller's own.
fn other_account(caller: Caller, account: AccountId) -> Result<AccountId, ApiError> {
    if caller.account == Some(account) {
        return Err(ApiError::Conflict);
    }
    Ok(account)
}

impl Handle<SetRole> for Site {
    async fn handle(&self, caller: Caller, change: NewRole) -> Result<Nothing, ApiError> {
        let account = other_account(caller, change.account)?;
        let changed = self
            .db(move |store| store.set_role(account, change.role))
            .await?;
        changed.then_some(Nothing {}).ok_or(ApiError::NotFound)
    }
}

impl Handle<Block> for Site {
    async fn handle(&self, caller: Caller, change: Blocking) -> Result<Nothing, ApiError> {
        let account = other_account(caller, change.account)?;
        let changed = self
            .db(move |store| store.set_blocked(account, change.blocked))
            .await?;
        changed.then_some(Nothing {}).ok_or(ApiError::NotFound)
    }
}

impl Handle<Accounts> for Site {
    async fn handle(&self, _: Caller, finding: Finding) -> Result<AccountsPage, ApiError> {
        let window = window(finding.after.as_deref(), ACCOUNTS_PAGE)?;
        // no words lists every account
        let query = match finding.query.trim() {
            "" => String::new(),
            words => line("query", words, QUERY_LIMIT)?,
        };
        let page = self.db(move |store| store.accounts(&query, window)).await?;
        Ok(AccountsPage {
            next: next(window, page.more),
            accounts: page.items,
        })
    }
}

impl Handle<ResetName> for Site {
    async fn handle(&self, _: Caller, channel: ByHandle) -> Result<Profile, ApiError> {
        let channel = self.channel_named(&channel.handle).await?;
        self.db(move |store| store.reset_name(channel))
            .await?
            .ok_or(ApiError::NotFound)
    }
}

impl Handle<RemovePhoto> for Site {
    async fn handle(&self, _: Caller, channel: ByHandle) -> Result<Profile, ApiError> {
        let channel = self.channel_named(&channel.handle).await?;
        self.replace_photo(channel, None).await
    }
}
