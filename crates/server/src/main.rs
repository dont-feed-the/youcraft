//! The `youcraft` binary: `serve` runs the site, `demo` fills an empty database with examples,
//! `transcode <input> <output>` is the child process that makes one upload playable, the
//! operator's commands ([`cli`]) act on its database, and `call` and `commands` ([`client`]) run
//! a site's commands from afar with a personal token.
//!
//! Configuration comes from the environment (see [`config`]). Under systemd the site serves the
//! socket it is handed (socket activation); otherwise it listens on `YOUCRAFT_ADDRESS`. It stops
//! on SIGTERM or Ctrl-C after the requests in flight are answered, as PID 1 of a container must.

// The server renders the pages of `youcraft_ui`, whose view types run deeper than the compiler's
// default limit allows, as that crate says.
#![recursion_limit = "256"]

#[cfg(test)]
mod channel_tests;
mod cli;
mod client;
mod config;
mod cookies;
mod demo;
mod dispatch;
mod google;
mod handlers;
mod http;
mod jobs;
mod listen;
mod mcp;
mod media;
#[cfg(test)]
mod page_tests;
mod photo;
mod rate;
mod secrets;
mod signin;
mod site;
mod storage;
mod transcode;
mod uploads;
mod visitor;

use std::process::ExitCode;
use std::sync::Arc;

use crate::config::Config;
use crate::site::Site;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("YOUCRAFT_LOG")
                .unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let (command, rest) = arguments
        .split_first()
        .map_or(("", &[][..]), |(c, r)| (c.as_str(), r));
    let result = match command {
        "serve" => serve(),
        "demo" => demo(),
        "transcode" => match rest {
            [input, output, memory, longest] => {
                transcode::run(input.as_ref(), output.as_ref(), memory, longest)
            }
            _ => Err(
                "usage: youcraft transcode <input> <output directory> <memory bytes> <longest seconds>"
                    .to_owned(),
            ),
        },
        "photo" => match rest {
            [input, output] => photo::run(input.as_ref(), output.as_ref()),
            _ => Err("usage: youcraft photo <input> <output directory>".to_owned()),
        },
        "call" => client::call(rest),
        "commands" => client::commands(rest),
        _ => operate(command, rest),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            tracing::error!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// Write `text` and a newline on standard output, failing rather than panicking as `println!`
/// does once the reader has gone (`youcraft commands | head`).
pub fn print(text: &str) -> Result<(), String> {
    use std::io::Write;
    writeln!(std::io::stdout().lock(), "{text}").map_err(|e| format!("standard output: {e}"))
}

fn serve() -> Result<(), String> {
    let config = Config::from_env()?;
    let site = Arc::new(Site::open(&config)?);
    let runtime = tokio::runtime::Runtime::new().map_err(|e| format!("tokio: {e}"))?;
    runtime.block_on(async {
        tokio::spawn(jobs::serve(Arc::clone(&site)));
        tokio::spawn(uploads::sweep(Arc::clone(&site)));
        http::serve(site, &config).await
    })
}

fn demo() -> Result<(), String> {
    let config = Config::from_env()?;
    let site = Site::open(&config)?;
    let runtime = tokio::runtime::Runtime::new().map_err(|e| format!("tokio: {e}"))?;
    runtime.block_on(demo::fill(&site))
}

fn operate(name: &str, arguments: &[String]) -> Result<(), String> {
    let (id, input) = cli::parse(name, arguments).map_err(|e| {
        format!(
            "{e}\nor: {}\nor: youcraft serve | youcraft demo",
            client::USAGE
        )
    })?;
    let config = Config::from_env()?;
    let site = Site::open(&config)?;
    let runtime = tokio::runtime::Runtime::new().map_err(|e| format!("tokio: {e}"))?;
    runtime.block_on(cli::run(&site, id, input))
}
