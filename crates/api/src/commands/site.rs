//! Commands about the site itself.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::{Command, Descriptor};
use crate::role::{Grant, Role};

/// Nothing: the input of a command that takes none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Nothing {}

/// List every command, with its schemas.
pub struct ListCommands;

impl Command for ListCommands {
    const ID: &'static str = "site.commands";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str =
        "List every command of the site, with the schemas of its input and output.";
    type Input = Nothing;
    type Output = Vec<Descriptor>;
}

/// What the site grants every account, whoever they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Policy {
    /// The least role of every signed-in account that is not blocked: `viewer` where only those
    /// invited publish, `creator` where everyone does.
    pub signed_in_role: Grant,
}

/// Tell what the site grants every account.
pub struct GetPolicy;

impl Command for GetPolicy {
    const ID: &'static str = "site.policy";
    const REQUIRES: Role = Role::Anyone;
    const SUMMARY: &'static str =
        "Tell what the site grants every account: the least role of a signed-in account.";
    type Input = Nothing;
    type Output = Policy;
}
