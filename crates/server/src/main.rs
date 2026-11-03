//! The `youcraft` binary: `serve` runs the site, `demo` fills an empty database with examples,
//! and the operator's commands ([`cli`]) act on its database.
//!
//! Configuration comes from the environment (see [`config`]). Under systemd the site serves the
//! socket it is handed (socket activation); otherwise it listens on `YOUCRAFT_ADDRESS`. It stops
//! on SIGTERM or Ctrl-C after the requests in flight are answered, as PID 1 of a container must.

mod cli;
mod config;
mod cookies;
mod demo;
mod dispatch;
mod google;
mod handlers;
mod http;
mod listen;
mod secrets;
mod signin;
mod site;
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
        .init();
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let (command, rest) = arguments
        .split_first()
        .map_or(("", &[][..]), |(c, r)| (c.as_str(), r));
    let result = match command {
        "serve" => serve(),
        "demo" => demo(),
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

fn serve() -> Result<(), String> {
    let config = Config::from_env()?;
    let site = Arc::new(Site::open(&config)?);
    let runtime = tokio::runtime::Runtime::new().map_err(|e| format!("tokio: {e}"))?;
    runtime.block_on(http::serve(site, &config))
}

fn demo() -> Result<(), String> {
    let config = Config::from_env()?;
    let site = Site::open(&config)?;
    demo::fill(site.store()).map_err(|e| format!("demo: {e}"))
}

fn operate(name: &str, arguments: &[String]) -> Result<(), String> {
    let (id, input) = cli::parse(name, arguments)
        .map_err(|e| format!("{e}\nor: youcraft serve | youcraft demo"))?;
    let config = Config::from_env()?;
    let site = Site::open(&config)?;
    let runtime = tokio::runtime::Runtime::new().map_err(|e| format!("tokio: {e}"))?;
    runtime.block_on(cli::run(&site, id, input))
}
