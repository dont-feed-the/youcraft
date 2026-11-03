//! The site's configuration, read from the environment.
//!
//! - `YOUCRAFT_DATA`: the data directory (database, media); `data` by default.
//! - `YOUCRAFT_SITE`: the browser bundle that `cargo xtask site` builds; `target/site` by default.
//! - `YOUCRAFT_ADDRESS`: where to listen without socket activation; `127.0.0.1:3100` by default.
//! - `YOUCRAFT_LOG`: what to log, as `tracing`'s filters write it; `info` by default.

use std::net::SocketAddr;
use std::path::PathBuf;

/// The configuration of one run.
#[derive(Clone, Debug)]
pub struct Config {
    /// The data directory.
    pub data: PathBuf,
    /// The browser bundle: `pkg/` and the files beside it.
    pub site: PathBuf,
    /// Where to listen when no socket is handed over.
    pub address: SocketAddr,
}

impl Config {
    /// The configuration the environment describes.
    pub fn from_env() -> Result<Config, String> {
        let path = |name: &str, default: &str| {
            std::env::var_os(name).map_or_else(|| PathBuf::from(default), PathBuf::from)
        };
        let address =
            std::env::var("YOUCRAFT_ADDRESS").unwrap_or_else(|_| "127.0.0.1:3100".to_owned());
        Ok(Config {
            data: path("YOUCRAFT_DATA", "data"),
            site: path("YOUCRAFT_SITE", "target/site"),
            address: address
                .parse()
                .map_err(|e| format!("YOUCRAFT_ADDRESS={address}: {e}"))?,
        })
    }
}
