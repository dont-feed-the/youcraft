//! Commands about the site itself.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::{Command, Descriptor};
use crate::role::Role;

/// Nothing: the input of a command that takes none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
