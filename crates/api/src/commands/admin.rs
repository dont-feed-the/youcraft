//! What the site's administrators do to accounts: list them, change their role, block and
//! unblock them, and take back a channel's name or photo that should not show.
//!
//! Accounts are people: an account is named by its identifier, and found by its name or by the
//! handles of the channels it belongs to, never by its e-mail address, which no list shows. No
//! administrator can change their own role or block themselves, so the site always keeps the one
//! acting.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::channel::Handle;
use crate::commands::profile::Profile;
use crate::commands::search::QUERY_LIMIT;
use crate::commands::site::Nothing;
use crate::ids::AccountId;
use crate::records::ChannelCard;
use crate::role::{Grant, Role};

/// Accounts per page of [`Accounts`].
pub const ACCOUNTS_PAGE: u32 = 50;

/// Which accounts to list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    /// Words the person's name, or the name or handle of one of their channels, holds, ignoring case, at most [`QUERY_LIMIT`]
    /// characters on one line; every account when empty.
    #[serde(default)]
    #[schemars(length(max = QUERY_LIMIT))]
    pub query: String,
    /// The `next` of the previous page; `None` for the first.
    #[serde(default)]
    pub after: Option<String>,
}

/// An account as the administrators see it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AccountRow {
    /// The account.
    pub account: AccountId,
    /// The person's name, as Google gave it when they first signed in.
    pub name: String,
    /// The channels they belong to, the one they joined first first.
    pub channels: Vec<ChannelCard>,
    /// Its own role, whatever the site grants every account.
    pub role: Grant,
    /// Whether it is blocked.
    pub blocked: bool,
    /// When it was made, in seconds since 1970.
    pub created_at: i64,
}

/// A page of accounts, the newest first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AccountsPage {
    /// The accounts.
    pub accounts: Vec<AccountRow>,
    /// Where the next page starts, if there is one.
    pub next: Option<String>,
}

/// A new role for an account.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewRole {
    /// The account.
    pub account: AccountId,
    /// The role it holds from now on.
    pub role: Grant,
}

/// Whether an account is blocked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Blocking {
    /// The account.
    pub account: AccountId,
    /// `true` to block it, `false` to unblock it.
    pub blocked: bool,
}

/// Change an account's role.
pub struct SetRole;

impl Command for SetRole {
    const ID: &'static str = "admin.set_role";
    const REQUIRES: Role = Role::Admin;
    const SUMMARY: &'static str = "Make an account a viewer, a creator or an administrator.";
    type Input = NewRole;
    type Output = Nothing;
    const WRITES: bool = true;
}

/// Block or unblock an account.
pub struct Block;

impl Command for Block {
    const ID: &'static str = "admin.block";
    const REQUIRES: Role = Role::Admin;
    const SUMMARY: &'static str =
        "Block an account from acting on the site, as any of its channels, or unblock it.";
    type Input = Blocking;
    type Output = Nothing;
    const WRITES: bool = true;
}

/// The site's accounts, the newest first.
pub struct Accounts;

impl Command for Accounts {
    const ID: &'static str = "admin.accounts";
    const REQUIRES: Role = Role::Admin;
    const SUMMARY: &'static str = "List the site's accounts, the newest first, 50 a page, with their channels, role and blocking; find them by name or by a channel's name or handle.";
    type Input = Finding;
    type Output = AccountsPage;
    const WRITES: bool = false;
}

/// Give a channel its handle back as its name.
pub struct ResetName;

impl Command for ResetName {
    const ID: &'static str = "admin.reset_name";
    const REQUIRES: Role = Role::Admin;
    const SUMMARY: &'static str =
        "Name a channel after its handle again, in place of a name that should not show.";
    type Input = Handle;
    type Output = Profile;
    const WRITES: bool = true;
}

/// Take a channel's photo away.
pub struct RemovePhoto;

impl Command for RemovePhoto {
    const ID: &'static str = "admin.remove_photo";
    const REQUIRES: Role = Role::Admin;
    const SUMMARY: &'static str =
        "Remove the photo of a channel that should not show, by its handle.";
    type Input = Handle;
    type Output = Profile;
    const WRITES: bool = true;
}
