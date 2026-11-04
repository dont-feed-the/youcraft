//! The site's configuration, read from the environment; [`Config::from_env`] names each
//! variable, what it sets and its default.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use youcraft_api::Grant;

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
    pub signed_in_role: Grant,
    /// The most channels one account belongs to, those it made and those it joined.
    pub channels_per_account: usize,
    /// What uploads may take.
    pub uploads: UploadLimits,
    /// How often one caller may change the site.
    pub writes: WriteRate,
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

/// What uploads may take: of the site's storage, of one account's share of it, and of time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UploadLimits {
    /// The largest file one may upload, in bytes.
    pub file_bytes: u64,
    /// The most bytes the site's videos may fill, uploads in progress included.
    pub storage_bytes: u64,
    /// The most uploads one account may have open at once.
    pub open: u64,
    /// The most bytes one account's open uploads may claim together.
    pub open_bytes: u64,
    /// How long an upload may take from its start.
    pub lifetime: Duration,
    /// The most pieces of uploads received at once.
    pub streams: usize,
}

/// How often one caller may change the site: at most `burst` changes at once, then one every
/// `interval` on average.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteRate {
    /// The most changes in a row.
    pub burst: u32,
    /// The time each change takes to be forgiven; `None` for no limit.
    pub interval: Option<Duration>,
}

/// What one transcode may use; the child process sets them on itself before it reads a byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscodeLimits {
    /// Address space, in bytes.
    pub memory_bytes: u64,
    /// Wall-clock time; the worker kills it after.
    pub wall: Duration,
    /// The longest video it makes playable.
    pub longest_video: Duration,
}

impl Config {
    /// The configuration the environment describes:
    ///
    /// - `YOUCRAFT_DATA`: the data directory (database, uploads, media); `data` by default.
    /// - `YOUCRAFT_SITE`: the browser bundle that `cargo xtask site` builds; `target/site` by
    ///   default.
    /// - `YOUCRAFT_ADDRESS`: where to listen without socket activation; `127.0.0.1:3100` by
    ///   default.
    /// - `YOUCRAFT_ORIGIN`: the scheme, host and port people reach the site at, which commands
    ///   posted by browsers must come from and Google sends them back to;
    ///   `http://localhost:3100` by default.
    /// - `YOUCRAFT_GOOGLE_CLIENT_ID`: the site's OAuth client ID at Google; without it nobody can
    ///   sign in.
    /// - `YOUCRAFT_GOOGLE_KEYS`: the file of Google's public signing keys (JWKS, as
    ///   `https://www.googleapis.com/oauth2/v3/certs` serves it), kept fresh by someone else,
    ///   since the site never reaches the network; needed with the client ID.
    /// - `YOUCRAFT_LOG`: what to log, as `tracing`'s filters write it; `info` by default.
    /// - `YOUCRAFT_SIGNED_IN_ROLE`: the least role of every account that is not blocked,
    ///   `viewer` or `creator`; `viewer` by default, so that only those invited publish.
    /// - `YOUCRAFT_CHANNELS_PER_ACCOUNT`: the most channels one account belongs to, those it
    ///   made and those it joined; 10 by default.
    /// - `YOUCRAFT_UPLOAD_BYTES`: the largest file one may upload; 16 GiB by default.
    /// - `YOUCRAFT_STORAGE_BYTES`: the most bytes the site's videos may fill, originals,
    ///   renditions and uploads in progress together; no limit by default.
    /// - `YOUCRAFT_OPEN_UPLOADS`: the most uploads one account may have open at once; 3 by
    ///   default.
    /// - `YOUCRAFT_OPEN_UPLOAD_BYTES`: the most bytes one account's open uploads may claim
    ///   together; the largest file by default.
    /// - `YOUCRAFT_UPLOAD_HOURS`: how long an upload may take from its start; then it lapses,
    ///   and its video and bytes are deleted; 24 by default.
    /// - `YOUCRAFT_UPLOAD_STREAMS`: the most pieces of uploads the site receives at once; 8 by
    ///   default.
    /// - `YOUCRAFT_WRITES_BURST`, `YOUCRAFT_WRITES_PER_MINUTE`: how often one caller (an
    ///   account, or an address when signed out) may change the site: a burst of 30, then 20 a
    ///   minute, by default; 0 a minute for no limit. Signing in counts as a change.
    /// - `YOUCRAFT_TRANSCODE_MEMORY_BYTES`: the address space a transcode may use; 8 GiB by
    ///   default.
    /// - `YOUCRAFT_TRANSCODE_HOURS`: the longest a transcode may run; 12 by default.
    /// - `YOUCRAFT_LONGEST_VIDEO_HOURS`: the longest video the site makes playable; 12 by default.
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
            Err(_) | Ok("viewer") => Grant::Viewer,
            Ok("creator") => Grant::Creator,
            Ok(other) => {
                return Err(format!(
                    "YOUCRAFT_SIGNED_IN_ROLE={other}: viewer or creator expected"
                ));
            }
        };
        let file_bytes = number("YOUCRAFT_UPLOAD_BYTES", 16 << 30)?;
        Ok(Config {
            data: path("YOUCRAFT_DATA", "data"),
            site: path("YOUCRAFT_SITE", "target/site"),
            address: address
                .parse()
                .map_err(|e| format!("YOUCRAFT_ADDRESS={address}: {e}"))?,
            origin,
            google,
            signed_in_role,
            channels_per_account: usize::try_from(number("YOUCRAFT_CHANNELS_PER_ACCOUNT", 10)?)
                .unwrap_or(usize::MAX),
            uploads: UploadLimits {
                file_bytes,
                storage_bytes: number("YOUCRAFT_STORAGE_BYTES", u64::MAX)?,
                open: number("YOUCRAFT_OPEN_UPLOADS", 3)?,
                open_bytes: number("YOUCRAFT_OPEN_UPLOAD_BYTES", file_bytes)?,
                lifetime: Duration::from_secs(
                    number("YOUCRAFT_UPLOAD_HOURS", 24)?.saturating_mul(3600),
                ),
                streams: usize::try_from(number("YOUCRAFT_UPLOAD_STREAMS", 8)?.max(1))
                    .unwrap_or(usize::MAX),
            },
            writes: WriteRate {
                burst: u32::try_from(number("YOUCRAFT_WRITES_BURST", 30)?.max(1))
                    .unwrap_or(u32::MAX),
                interval: Duration::from_secs(60).checked_div(
                    u32::try_from(number("YOUCRAFT_WRITES_PER_MINUTE", 20)?).unwrap_or(u32::MAX),
                ),
            },
            transcode: TranscodeLimits {
                memory_bytes: number("YOUCRAFT_TRANSCODE_MEMORY_BYTES", 8 << 30)?,
                wall: Duration::from_secs(
                    number("YOUCRAFT_TRANSCODE_HOURS", 12)?.saturating_mul(3600),
                ),
                longest_video: Duration::from_secs(
                    number("YOUCRAFT_LONGEST_VIDEO_HOURS", 12)?.saturating_mul(3600),
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

    /// The photos of channels: a directory per channel, holding one per version.
    pub fn photos(&self) -> PathBuf {
        self.data.join("photos")
    }

    /// The media directories of the videos that play, one per video.
    pub fn media(&self) -> PathBuf {
        self.data.join("media")
    }

    /// A configuration for tests, over the data directory `data`: files up to 1 MiB, no storage
    /// limit, Google unset, every account a viewer at least, and writes unlimited.
    #[cfg(test)]
    pub fn testing(data: PathBuf) -> Config {
        Config {
            site: data.join("site"),
            data,
            address: SocketAddr::from(([127, 0, 0, 1], 0)),
            origin: "https://youcraft.test".to_owned(),
            google: None,
            signed_in_role: Grant::Viewer,
            channels_per_account: 3,
            uploads: UploadLimits {
                file_bytes: 1 << 20,
                storage_bytes: u64::MAX,
                open: 3,
                open_bytes: 1 << 20,
                lifetime: Duration::from_secs(3600),
                streams: 2,
            },
            writes: WriteRate {
                burst: 1,
                interval: None,
            },
            transcode: TranscodeLimits {
                memory_bytes: 1 << 30,
                wall: Duration::from_secs(60),
                longest_video: Duration::from_secs(3600),
            },
        }
    }
}
