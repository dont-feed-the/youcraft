//! The `youcraft` binary as a client of a site, for programs and people at a terminal.
//!
//! - `youcraft call <command id> [<input JSON>] [--site <URL>]` posts one command to the site's
//!   `/api/v1/<id>` (`https://youcraft.tube` unless `--site` names another), as the owner of the
//!   personal token in `YOUCRAFT_TOKEN`, or signed out without one, and prints its JSON answer.
//! - `youcraft commands` lists the commands this build knows, with the role each requires;
//!   `youcraft commands <id>` prints one with the schemas of its input and output.

use std::time::Duration;

use serde_json::Value;
use youcraft_api::command::{Descriptor, descriptors};
use youcraft_api::routes;

/// How to call them.
pub const USAGE: &str = "youcraft call <command> [<json>] [--site <url>] | \
                         youcraft commands [<command>]";
/// The site `call` reaches unless told another.
const SITE: &str = "https://youcraft.tube";
/// The variable that holds the personal token.
const TOKEN: &str = "YOUCRAFT_TOKEN";
/// How long a call may take, answer included.
const PATIENCE: Duration = Duration::from_secs(120);

/// What `call` was asked to do.
#[derive(Debug, PartialEq, Eq)]
struct Call {
    /// The site's origin, without a trailing `/`.
    site: String,
    /// The command.
    id: String,
    /// Its input, as JSON text.
    input: String,
}

/// The call `arguments` (after `call`) ask for.
fn parse_call(arguments: &[String]) -> Result<Call, String> {
    let usage = || format!("usage: {USAGE}");
    let mut site = SITE.to_owned();
    let mut positional = Vec::new();
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        if argument == "--site" {
            site.clone_from(rest.next().ok_or_else(usage)?);
        } else if let Some(url) = argument.strip_prefix("--site=") {
            url.clone_into(&mut site);
        } else {
            positional.push(argument.as_str());
        }
    }
    let (id, input) = match positional.as_slice() {
        [id] => (*id, "{}"),
        [id, input] => (*id, *input),
        _ => return Err(usage()),
    };
    let well_formed = !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b == b'_' || b == b'.');
    if !well_formed {
        return Err(format!("{id}: a command id is area.verb"));
    }
    serde_json::from_str::<Value>(input).map_err(|e| format!("input: {e}"))?;
    if !(site.starts_with("https://") || site.starts_with("http://")) {
        return Err(format!("{site}: the site is an http or https URL"));
    }
    Ok(Call {
        site: site.trim_end_matches('/').to_owned(),
        id: id.to_owned(),
        input: input.to_owned(),
    })
}

/// `youcraft call`: run the command over HTTP and print its answer, or fail with its error.
pub fn call(arguments: &[String]) -> Result<(), String> {
    let call = parse_call(arguments)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(PATIENCE))
        .build()
        .into();
    let mut request = agent
        .post(format!("{}{}", call.site, routes::command(&call.id)))
        .header("content-type", "application/json");
    if let Some(token) = std::env::var(TOKEN).ok().filter(|token| !token.is_empty()) {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let mut response = request
        .send(call.input.as_str())
        .map_err(|e| format!("{}: {e}", call.site))?;
    let status = response.status();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("{}: {e}", call.site))?;
    let answer = serde_json::from_str::<Value>(&body).map_or(body, |json| format!("{json:#}"));
    if status.is_success() {
        crate::print(&answer)
    } else {
        Err(format!("{} ({status}): {answer}", call.id))
    }
}

/// `youcraft commands`: every command, or the one `arguments` names in full.
pub fn commands(arguments: &[String]) -> Result<(), String> {
    match arguments {
        [] => descriptors()
            .iter()
            .try_for_each(|command| crate::print(&line(command))),
        [id] => {
            let command = descriptors()
                .into_iter()
                .find(|command| &command.id == id)
                .ok_or_else(|| format!("{id}: no such command"))?;
            let json = serde_json::to_value(&command).map_err(|e| e.to_string())?;
            crate::print(&format!("{json:#}"))
        }
        _ => Err(format!("usage: {USAGE}")),
    }
}

/// `command` on one line: id, role, summary.
fn line(command: &Descriptor) -> String {
    format!(
        "{:<24} {:<8} {}",
        command.id,
        command.requires.as_str(),
        command.summary
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(text: &[&str]) -> Vec<String> {
        text.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn calls_name_a_command_its_input_and_a_site() {
        assert_eq!(
            parse_call(&arguments(&["feed.home"])).unwrap(),
            Call {
                site: SITE.to_owned(),
                id: "feed.home".to_owned(),
                input: "{}".to_owned()
            }
        );
        let call = parse_call(&arguments(&[
            "--site",
            "http://localhost:3100/",
            "video.get",
            r#"{"video": "AAAAAAAAAAA"}"#,
        ]))
        .unwrap();
        assert_eq!(call.site, "http://localhost:3100");
        assert_eq!(call.input, r#"{"video": "AAAAAAAAAAA"}"#);
        assert!(parse_call(&arguments(&["../health"])).is_err());
        assert!(parse_call(&arguments(&["feed.home", "{"])).is_err());
        assert!(parse_call(&arguments(&["feed.home", "--site=ftp://x"])).is_err());
        assert!(parse_call(&arguments(&[])).is_err());
    }

    #[test]
    fn every_command_is_listed_with_its_role() {
        assert!(line(&Descriptor::of::<youcraft_api::commands::feed::Home>()).contains("anyone"));
        assert!(commands(&arguments(&["no.such"])).is_err());
    }
}
