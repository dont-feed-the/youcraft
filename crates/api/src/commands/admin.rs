//! What the site's administrators do to accounts: change their role, block and unblock them.
//!
//! Accounts are named by their channel's handle, the name everyone sees. No administrator can
//! change their own role or block themselves, so the site always keeps the one acting.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::site::Nothing;
use crate::role::Role;

/// A new role for an account.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewRole {
    /// The handle of the account's channel, with or without `@`.
    pub handle: String,
    /// [`Role::Viewer`], [`Role::Creator`] or [`Role::Admin`].
    pub role: Role,
}

/// Whether an account is blocked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Blocking {
    /// The handle of the account's channel, with or without `@`.
    pub handle: String,
    /// `true` to block it, `false` to unblock it.
    pub blocked: bool,
}

/// Change an account's role.
pub struct SetRole;

impl Command for SetRole {
    const ID: &'static str = "admin.set_role";
    const REQUIRES: Role = Role::Admin;
    const SUMMARY: &'static str =
        "Make an account a viewer, a creator or an administrator, by its channel's handle.";
    type Input = NewRole;
    type Output = Nothing;
}

/// Block or unblock an account.
pub struct Block;

impl Command for Block {
    const ID: &'static str = "admin.block";
    const REQUIRES: Role = Role::Admin;
    const SUMMARY: &'static str =
        "Block an account from acting on the site, or unblock it, by its channel's handle.";
    type Input = Blocking;
    type Output = Nothing;
}
