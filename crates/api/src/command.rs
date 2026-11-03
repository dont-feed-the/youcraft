//! What a command is: an id, typed input and output, the role it requires, a one-line summary.
//!
//! Every command type is listed once, in [`crate::for_each_command`]; the server builds its
//! dispatch table from that list, so a command without a handler does not compile, and the
//! command line and the MCP server describe the site from [`descriptors`].

use std::future::Future;
use std::pin::Pin;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::role::Role;

/// One action on the site, run the same way by pages, programs and agents.
pub trait Command: 'static {
    /// Its id, `area.verb`, as in `POST /api/v1/<id>`.
    const ID: &'static str;
    /// The least role a caller needs; the command itself may refuse more (another's video).
    const REQUIRES: Role;
    /// What it does, in one English sentence, for the command line and agents.
    const SUMMARY: &'static str;
    /// What the caller sends.
    type Input: Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static;
    /// What it answers.
    type Output: Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static;
}

/// A command as the command line and agents see it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Descriptor {
    /// See [`Command::ID`].
    pub id: String,
    /// See [`Command::REQUIRES`].
    pub requires: Role,
    /// See [`Command::SUMMARY`].
    pub summary: String,
    /// The JSON Schema of its input.
    pub input: serde_json::Value,
    /// The JSON Schema of its output.
    pub output: serde_json::Value,
}

impl Descriptor {
    /// The descriptor of `C`.
    pub fn of<C: Command>() -> Descriptor {
        Descriptor {
            id: C::ID.to_owned(),
            requires: C::REQUIRES,
            summary: C::SUMMARY.to_owned(),
            input: schemars::schema_for!(C::Input).to_value(),
            output: schemars::schema_for!(C::Output).to_value(),
        }
    }
}

/// A command's answer on its way, as JSON.
pub type Answer = Pin<Box<dyn Future<Output = Result<serde_json::Value, ApiError>> + Send>>;

/// Something that runs commands by id for one caller: the server, for the pages it renders.
pub trait Dispatch: Send + Sync {
    /// Run the command `id` on `input`.
    fn dispatch(&self, id: &str, input: serde_json::Value) -> Answer;
}

macro_rules! descriptors_of {
    ($($command:path),* $(,)?) => {
        vec![$(Descriptor::of::<$command>()),*]
    };
}

/// Every command of the site, in the order [`crate::for_each_command`] lists them.
pub fn descriptors() -> Vec<Descriptor> {
    crate::for_each_command!(descriptors_of)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn ids_are_unique_and_well_formed() {
        let all = descriptors();
        let ids: BTreeSet<&str> = all.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids.len(), all.len(), "two commands share an id");
        for id in ids {
            let (area, verb) = id
                .split_once('.')
                .unwrap_or_else(|| panic!("{id}: area.verb"));
            for part in [area, verb] {
                assert!(
                    !part.is_empty() && part.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                    "{id}: lowercase letters and _ only"
                );
            }
        }
    }

    #[test]
    fn every_summary_is_one_sentence() {
        for descriptor in descriptors() {
            let summary = &descriptor.summary;
            assert!(
                summary.ends_with('.') && !summary.contains('\n'),
                "{}: {summary}",
                descriptor.id
            );
        }
    }
}
