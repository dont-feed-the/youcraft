//! Personal tokens: how a program acts as the person who made it, with exactly their rights.
//!
//! A program sends its token as `Authorization: Bearer <secret>`. The site keeps only a hash of
//! the secret, so it is shown once, when the token is created. Tokens are their owner's to manage
//! even while the owner is blocked; a blocked owner's tokens can then do only what the owner can.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands::site::Nothing;
use crate::ids::TokenId;
use crate::role::Role;

/// A token as its owner lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Token {
    /// Its identifier.
    pub id: TokenId,
    /// What its owner called it.
    pub name: String,
    /// When it was made, in seconds since 1970.
    pub created_at: i64,
    /// When a request last used it, in seconds since 1970.
    pub last_used_at: Option<i64>,
}

/// The name of a new token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Naming {
    /// What to call it: 1 to [`NAME_LIMIT`] characters, on one line.
    #[schemars(length(min = 1, max = NAME_LIMIT))]
    pub name: String,
}

/// A new token, with the secret that is never shown again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Created {
    /// The token.
    pub token: Token,
    /// What the program sends as `Authorization: Bearer <secret>`.
    pub secret: String,
}

/// One token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Which {
    /// Its identifier.
    pub token: TokenId,
}

/// The most characters a token's name has.
pub const NAME_LIMIT: usize = 100;

/// The most tokens one account keeps.
pub const TOKEN_LIMIT: usize = 50;

/// Make a token.
pub struct Create;

impl Command for Create {
    const ID: &'static str = "token.create";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str =
        "Make a personal token that acts as the signed-in person; its secret is shown only now.";
    type Input = Naming;
    type Output = Created;
}

/// List one's tokens.
pub struct List;

impl Command for List {
    const ID: &'static str = "token.list";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str = "List the signed-in person's personal tokens, newest first.";
    type Input = Nothing;
    type Output = Vec<Token>;
}

/// Revoke a token.
pub struct Revoke;

impl Command for Revoke {
    const ID: &'static str = "token.revoke";
    const REQUIRES: Role = Role::SignedIn;
    const SUMMARY: &'static str = "Revoke one of the signed-in person's personal tokens.";
    type Input = Which;
    type Output = Nothing;
}
