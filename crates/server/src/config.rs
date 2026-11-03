//! The site's configuration, read from the environment.
//!
//! - `YOUCRAFT_DATA`: the data directory (database, uploads, media); `data` by default.
//! - `YOUCRAFT_SITE`: the browser bundle that `cargo xtask site` builds; `target/site` by default.
//! - `YOUCRAFT_ADDRESS`: where to listen without socket activation; `127.0.0.1:3100` by default.
//! - `YOUCRAFT_ORIGIN`: the scheme, host and port people reach the site at, which commands posted
//!   by browsers must come from and Google sends them back to; `http://localhost:3100` by default.
//! - `YOUCRAFT_GOOGLE_CLIENT_ID`: the site's OAuth client ID at Google; without it nobody can
//!   sign in.
//! - `YOUCRAFT_GOOGLE_KEYS`: the file of Google's public signing keys (JWKS, as
//!   `https://www.googleapis.com/oauth2/v3/certs` serves it), kept fresh by someone else, since
//!   the site never reaches the network; needed with the client ID.
//! - `YOUCRAFT_LOG`: what to log, as `tracing`'s filters write it; `info` by default.
//! - `YOUCRAFT_SIGNED_IN_ROLE`: the least role of every account that is not blocked, `viewer`
//!   or `creator`; `viewer` by default, so that only those invited publish.
//! - `YOUCRAFT_UPLOAD_BYTES`: the largest file one may upload; 16 GiB by default.
//! - `YOUCRAFT_STORAGE_BYTES`: the most bytes the site's videos may fill, originals, renditions
//!   and uploads in progress together; no limit by default.
//! - `YOUCRAFT_TRANSCODE_MEMORY_BYTES`: the address space a transcode may use; 8 GiB by default.
//! - `YOUCRAFT_TRANSCODE_HOURS`: the longest a transcode may run; 12 by default.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use youcraft_api::Role;

/// The configuration of one run.
#[derive(Clone, Debug)]
pub struct Config {
    /// The data directory.
    pub data: PathBuf,
    /// The browser bundle: `pkg/` and the files beside it.
    pub site: PathBuf,
    /// Where to listen when no socket is handed over.
    pub address: SocketAddr,
    /// Where people reach the site, as `https://host[:port]`, with no path.
    pub origin: String,
    /// Signing in with Google, if it is set up.
    pub google: Option<Google>,
    /// The least role of every account that is not blocked.
    pub signed_in_role: Role,
    /// The largest file one may upload, in bytes.
    pub upload_bytes: u64,
    /// The most bytes the site's videos may fill.
    pub storage_bytes: u64,
    /// What one transcode may use.
    pub transcode: TranscodeLimits,
}

/// The site as Google knows it.
#[derive(Clone, Debug)]
pub struct Google {
    /// The OAuth client ID, which ID tokens name as their audience.
    pub client_id: String,
    /// The file of Google's public signing keys.
    pub keys: PathBuf,
}

/// What one transcode may use; the child process sets them on itself before it reads a byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscodeLimits {
    /// Address space, in bytes.
    pub memory_bytes: u64,
    /// Wall-clock time; the worker kills it after.
    pub wall: Duration,
}

impl Config {
    /// The configuration the environment describes.
    pub fn from_env() -> Result<Config, String> {
        let path = |name: &str, default: &str| {
            std::env::var_os(name).map_or_else(|| PathBuf::from(default), PathBuf::from)
        };
        let number = |name: &str, default: u64| -> Result<u64, String> {
            match std::env::var(name) {
                Ok(text) => text.parse().map_err(|e| format!("{name}={text}: {e}")),
                Err(_) => Ok(default),
            }
        };
        let address =
            std::env::var("YOUCRAFT_ADDRESS").unwrap_or_else(|_| "127.0.0.1:3100".to_owned());
        let origin =
            std::env::var("YOUCRAFT_ORIGIN").unwrap_or_else(|_| "http://localhost:3100".to_owned());
        if !(origin.starts_with("https://") || origin.starts_with("http://"))
            || origin.matches('/').count() != 2
        {
            return Err(format!(
                "YOUCRAFT_ORIGIN={origin}: scheme://host[:port] expected"
            ));
        }
        let google = match (
            std::env::var("YOUCRAFT_GOOGLE_CLIENT_ID"),
            std::env::var_os("YOUCRAFT_GOOGLE_KEYS"),
        ) {
            (Ok(client_id), Some(keys)) if !client_id.is_empty() => Some(Google {
                client_id,
                keys: PathBuf::from(keys),
            }),
            (Ok(client_id), None) if !client_id.is_empty() => {
                return Err("YOUCRAFT_GOOGLE_CLIENT_ID needs YOUCRAFT_GOOGLE_KEYS".to_owned());
            }
            _ => None,
        };
        let signed_in_role = match std::env::var("YOUCRAFT_SIGNED_IN_ROLE").as_deref() {
            Err(_) | Ok("viewer") => Role::Viewer,
            Ok("creator") => Role::Creator,
            Ok(other) => {
                return Err(format!(
                    "YOUCRAFT_SIGNED_IN_ROLE={other}: viewer or creator expected"
                ));
            }
        };
        Ok(Config {
            data: path("YOUCRAFT_DATA", "data"),
            site: path("YOUCRAFT_SITE", "target/site"),
            address: address
                .parse()
                .map_err(|e| format!("YOUCRAFT_ADDRESS={address}: {e}"))?,
            origin,
            google,
            signed_in_role,
            upload_bytes: number("YOUCRAFT_UPLOAD_BYTES", 16 << 30)?,
            storage_bytes: number("YOUCRAFT_STORAGE_BYTES", u64::MAX)?,
            transcode: TranscodeLimits {
                memory_bytes: number("YOUCRAFT_TRANSCODE_MEMORY_BYTES", 8 << 30)?,
                wall: Duration::from_secs(
                    number("YOUCRAFT_TRANSCODE_HOURS", 12)?.saturating_mul(3600),
                ),
            },
        })
    }

    /// Where uploads in progress are written, one file per video.
    pub fn uploads(&self) -> PathBuf {
        self.data.join("uploads")
    }

    /// Where uploaded files are kept once whole, one per video.
    pub fn originals(&self) -> PathBuf {
        self.data.join("originals")
    }

    /// Where a transcode writes, one directory per video, before its media is served.
    pub fn work(&self) -> PathBuf {
        self.data.join("work")
    }

    /// The media directories of the videos that play, one per video.
    pub fn media(&self) -> PathBuf {
        self.data.join("media")
    }
}
