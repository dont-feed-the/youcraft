//! Pictures people give the site, each made by a child process: channels' photos, `youcraft photo
//! <input> <output>`, and the thumbnails chosen for videos, `youcraft thumbnail <input> <output>`;
//! and what the server does with them.
//!
//! The child limits itself before reading a byte (address space, CPU time, the size of each
//! file it writes), decodes the picture, and writes its sizes into the output directory; a
//! picture that breaks the decoder kills only the child. It reports its outcome as one line on
//! standard output, `made` or `failure <kind>`. The server writes the uploaded bytes to a work
//! file, runs the child on them under a wall-clock limit, then moves the made directory to its
//! place, `photos/<channel>/<version>/` or `<version>` among the thumbnails in a video's media: a
//! fresh version each time, so every address keeps its bytes.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use base64::Engine;
use base64::alphabet::STANDARD;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use rlimit::Resource;
use tokio::io::AsyncReadExt;
use youcraft_api::commands::profile::PHOTO_BYTES;
use youcraft_api::commands::video::THUMBNAIL_BYTES;
use youcraft_api::routes::media_files;
use youcraft_api::{ApiError, ChannelId, Rule, VideoId};
use youcraft_store::profiles::PhotoFiles;
use youcraft_transcode::uploaded;

use crate::site::Site;
use crate::storage;

/// The address space the child may use: the decoder's own cap, and room for the program.
const MEMORY_BYTES: u64 = uploaded::MAX_DECODED + (256 << 20);
/// The CPU seconds the child may spend.
const CPU_SECONDS: u64 = 30;
/// The largest file the child may write.
const FILE_BYTES: u64 = 4 << 20;
/// How long the server waits for the child.
const WALL: Duration = Duration::from_secs(60);

/// Base64 as RFC 4648 writes it, with its padding or without.
const BASE64: GeneralPurpose = GeneralPurpose::new(
    &STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

fn limit(resource: Resource, value: u64) -> Result<(), String> {
    rlimit::setrlimit(resource, value, value).map_err(|e| format!("limit {resource:?}: {e}"))
}

/// Write `line` to standard output; the parent that reads it may be gone, which changes nothing.
fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// What the child makes of an uploaded picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Making {
    /// A channel's photo.
    Photo,
    /// The thumbnail chosen for a video.
    Thumbnail,
}

impl Making {
    /// The command of the `youcraft` binary that makes it.
    fn command(self) -> &'static str {
        match self {
            Making::Photo => "photo",
            Making::Thumbnail => "thumbnail",
        }
    }

    /// The most bytes its file may have.
    fn limit(self) -> u64 {
        let limit = match self {
            Making::Photo => PHOTO_BYTES,
            Making::Thumbnail => THUMBNAIL_BYTES,
        };
        u64::try_from(limit).unwrap_or(u64::MAX)
    }
}

/// Make the picture in the file `input` into the directory `output`, under limits.
pub fn run(making: Making, input: &Path, output: &Path) -> Result<(), String> {
    let result = limit(Resource::AS, MEMORY_BYTES)
        .and_then(|()| limit(Resource::CPU, CPU_SECONDS))
        .and_then(|()| limit(Resource::FSIZE, FILE_BYTES))
        .map_err(youcraft_transcode::Error::Io)
        .and_then(|()| {
            let bytes = std::fs::read(input)
                .map_err(|e| youcraft_transcode::Error::Io(format!("{}: {e}", input.display())))?;
            match making {
                Making::Photo => uploaded::photo(&bytes, output),
                Making::Thumbnail => uploaded::thumbnail(&bytes, output),
            }
        });
    match result {
        Ok(()) => {
            say("made");
            Ok(())
        }
        Err(error) => {
            say(&format!("failure {}", error.failure().as_str()));
            Err(format!("{}: {error}", making.command()))
        }
    }
}

/// The directory of the photo version `version` of `channel`.
pub fn photo_directory(site: &Site, channel: ChannelId, version: &str) -> PathBuf {
    site.config()
        .photos()
        .join(channel.to_string())
        .join(version)
}

/// The directory of the thumbnail version `version` chosen for `video`.
pub fn thumbnail_directory(site: &Site, video: VideoId, version: &str) -> PathBuf {
    site.config()
        .media()
        .join(video.to_string())
        .join(media_files::chosen_thumbnail(version))
}

/// Whether `version` is one [`make`] draws: sixteen lowercase hexadecimal digits.
pub fn is_version(version: &str) -> bool {
    version.len() == 16
        && version
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A picture made, waiting in the work directory for its place.
pub struct Made {
    version: String,
    work: PathBuf,
}

impl Made {
    /// The version it was made as.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Move its files to the directory `target`.
    pub async fn place(self, target: &Path) -> Result<(), ApiError> {
        let placed = match target.parent() {
            Some(parent) => tokio::fs::create_dir_all(parent).await,
            None => Ok(()),
        };
        let placed = match placed {
            Ok(()) => tokio::fs::rename(&self.work, target).await,
            Err(error) => Err(error),
        };
        if let Err(error) = placed {
            let _ = tokio::fs::remove_dir_all(&self.work).await;
            tracing::error!("picture {}: place: {error}", self.version);
            return Err(ApiError::Internal);
        }
        Ok(())
    }

    /// Forget its files.
    pub async fn discard(self) {
        let _ = tokio::fs::remove_dir_all(&self.work).await;
    }
}

/// Make a picture from `image`, base64 as a command carries it, into the work directory.
pub async fn make(site: &Site, making: Making, image: &str) -> Result<Made, ApiError> {
    let limit = making.limit();
    // base64 is 4 characters per 3 bytes: refuse before decoding what cannot fit
    if u64::try_from(image.len()).unwrap_or(u64::MAX) / 4 * 3 > limit.saturating_add(3) {
        return Err(ApiError::TooLarge { limit });
    }
    let bytes = BASE64
        .decode(image.trim())
        .map_err(|_| ApiError::invalid("image", Rule::Malformed))?;
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if bytes.is_empty() {
        return Err(ApiError::invalid("image", Rule::Required));
    }
    if size > limit {
        return Err(ApiError::TooLarge { limit });
    }
    // the sizes made are a few hundred kilobytes at most; a full site takes no new picture
    if storage::room(site).await? < size {
        return Err(ApiError::TooLarge { limit: 0 });
    }
    let version = format!("{:016x}", crate::secrets::random_bits());
    let work = site
        .config()
        .work()
        .join(format!("{}-{version}", making.command()));
    let input = work.with_extension("in");
    tokio::fs::write(&input, &bytes).await.map_err(|error| {
        tracing::error!("picture {version}: write: {error}");
        ApiError::Internal
    })?;
    let made = child(making, &input, &work).await;
    let _ = tokio::fs::remove_file(&input).await;
    if let Err(error) = made {
        let _ = tokio::fs::remove_dir_all(&work).await;
        return Err(error);
    }
    Ok(Made { version, work })
}

/// Make a photo of `channel` from `image`, base64 as the command carries it; its new files.
pub async fn make_photo(
    site: &Site,
    channel: ChannelId,
    image: &str,
) -> Result<PhotoFiles, ApiError> {
    let made = make(site, Making::Photo, image).await?;
    let version = made.version().to_owned();
    let target = photo_directory(site, channel, &version);
    made.place(&target).await?;
    let bytes = storage::walk(move || storage::bytes_under(&target)).await?;
    Ok(PhotoFiles { version, bytes })
}

/// Run the child on `input` into `output`, and read its outcome.
async fn child(making: Making, input: &Path, output: &Path) -> Result<(), ApiError> {
    let internal = |detail: String| {
        tracing::error!("{}: {detail}", making.command());
        ApiError::Internal
    };
    let program = std::env::current_exe().map_err(|e| internal(format!("own path: {e}")))?;
    let mut child = tokio::process::Command::new(program)
        .arg(making.command())
        .arg(input)
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| internal(format!("start: {e}")))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| internal("no output".to_owned()))?;
    let follow = async {
        let mut said = String::new();
        // the child says one short line; more is not read
        let _ = (&mut stdout).take(256).read_to_string(&mut said).await;
        (said, child.wait().await)
    };
    let refused = ApiError::invalid("image", Rule::Malformed);
    match tokio::time::timeout(WALL, follow).await {
        Err(_) => Err(internal("took longer than allowed".to_owned())),
        Ok((said, Ok(status))) if status.success() && said.trim() == "made" => Ok(()),
        Ok((said, _)) if said.starts_with("failure internal") => {
            Err(internal("the child failed".to_owned()))
        }
        Ok(_) => Err(refused),
    }
}

/// Forget the files of the photo version `version` of `channel`, if it is one.
pub async fn remove_photo(site: &Site, channel: ChannelId, version: Option<String>) {
    if let Some(version) = version.filter(|v| is_version(v)) {
        let _ = tokio::fs::remove_dir_all(photo_directory(site, channel, &version)).await;
    }
}

/// Forget the files of the thumbnail version `version` chosen for `video`, if it is one.
pub async fn remove_thumbnail(site: &Site, video: VideoId, version: Option<String>) {
    if let Some(version) = version.filter(|v| is_version(v)) {
        let _ = tokio::fs::remove_dir_all(thumbnail_directory(site, video, &version)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_sixteen_hexadecimal_digits() {
        assert!(is_version("0123456789abcdef"));
        for wrong in ["", "0123456789ABCDEF", "../../../../etc", "0123456789abcde"] {
            assert!(!is_version(wrong), "{wrong}");
        }
        assert_eq!(BASE64.decode("aGk").unwrap(), b"hi");
        assert_eq!(BASE64.decode("aGk=").unwrap(), b"hi");
    }
}
