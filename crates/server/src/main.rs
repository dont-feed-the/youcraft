//! The `youcraft` binary: `serve` runs the site, `demo` fills an empty database with examples.
//!
//! Configuration comes from the environment (see [`config`]). Under systemd the site serves the
//! socket it is handed (socket activation); otherwise it listens on `YOUCRAFT_ADDRESS`. It stops
//! on SIGTERM or Ctrl-C after the requests in flight are answered, as PID 1 of a container must.

mod config;
mod demo;
mod dispatch;
mod handlers;
mod http;
mod listen;
mod site;

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
    let command = std::env::args().nth(1).unwrap_or_default();
    let result = match command.as_str() {
        "serve" => serve(),
        "demo" => demo(),
        _ => Err("usage: youcraft serve | youcraft demo".to_owned()),
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
