//! The operator's commands: what whoever runs the server does from its machine, such as making
//! the first administrator. Each runs a command of the site as [`Caller::OPERATOR`], on the
//! database directly, and prints its answer as JSON.
//!
//! - `youcraft invite <email> [creator|admin]`: invite an address (as a creator by default).
//! - `youcraft admin <email>`: make an address an administrator.
//! - `youcraft block <@handle>` and `youcraft unblock <@handle>`.
//! - `youcraft run <command id> [<input JSON>]`: any command, as the operator.

use serde_json::{Value, json};
use youcraft_api::Command;
use youcraft_api::commands::{admin, invite};

use crate::dispatch;
use crate::site::{Caller, Site};

/// How to call them.
pub const USAGE: &str = "youcraft invite <email> [creator|admin] | youcraft admin <email> | \
                         youcraft block <@handle> | youcraft unblock <@handle> | \
                         youcraft run <command> [<json>]";

/// The command and input `arguments` (after the subcommand's name `name`) ask for.
pub fn parse(name: &str, arguments: &[String]) -> Result<(&'static str, Value), String> {
    let first = arguments.first().map(String::as_str);
    let second = arguments.get(1).map(String::as_str);
    let usage = || format!("usage: {USAGE}");
    match (name, first, second) {
        ("invite", Some(email), role) => Ok((
            invite::Create::ID,
            json!({"email": email, "role": role.unwrap_or("creator")}),
        )),
        ("admin", Some(email), None) => {
            Ok((invite::Create::ID, json!({"email": email, "role": "admin"})))
        }
        ("block" | "unblock", Some(handle), None) => Ok((
            admin::Block::ID,
            json!({"handle": handle, "blocked": name == "block"}),
        )),
        ("run", Some(id), input) => {
            let id = youcraft_api::command::descriptors()
                .into_iter()
                .find(|descriptor| descriptor.id == id)
                .map(|descriptor| descriptor.id)
                .ok_or_else(|| format!("{id}: no such command"))?;
            let input =
                serde_json::from_str(input.unwrap_or("{}")).map_err(|e| format!("input: {e}"))?;
            Ok((leak(id), input))
        }
        _ => Err(usage()),
    }
}

fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

/// Run the command `id` on `input` as the operator, and print its answer.
pub async fn run(site: &Site, id: &str, input: Value) -> Result<(), String> {
    let answer = dispatch::run(site, Caller::OPERATOR, id, input)
        .await
        .map_err(|error| format!("{id}: {error}"))?;
    println!("{answer:#}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(text: &[&str]) -> Vec<String> {
        text.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn operator_commands_are_site_commands() {
        assert_eq!(
            parse("admin", &arguments(&["a@example.com"])).unwrap(),
            (
                "invite.create",
                json!({"email": "a@example.com", "role": "admin"})
            )
        );
        assert_eq!(
            parse("unblock", &arguments(&["@ada"])).unwrap(),
            ("admin.block", json!({"handle": "@ada", "blocked": false}))
        );
        assert_eq!(
            parse("run", &arguments(&["feed.home"])).unwrap().0,
            "feed.home"
        );
        assert!(parse("run", &arguments(&["no.such"])).is_err());
        assert!(parse("block", &arguments(&[])).is_err());
    }
}
