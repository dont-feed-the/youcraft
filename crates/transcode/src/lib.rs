//! Uploads made into what the site serves: H.264 renditions and an AAC track as CMAF files,
//! their HLS playlists, thumbnails and a storyboard, through FilmCraft's codecs.
//!
//! [`transcode`] reads any file FilmCraft opens (MP4, MOV, Matroska, WebM, MXF, AVI, MPEG-TS…;
//! H.264, HEVC, VP8, VP9, AV1, ProRes…) and writes a video's media directory: the files named
//! by `youcraft_api::routes::media_files` and the track files they refer to. The server runs it
//! in a child process with resource limits, so a file that breaks a decoder breaks nothing else.
//! [`photo`] makes a channel's photo the same way. [`synthetic`] makes videos with known frames
//! and tones for the demo and the tests.

pub mod audio;
pub mod cmaf;
pub mod hls;
pub mod ladder;
pub mod picture;
pub mod pipeline;
pub mod stills;
pub mod synthetic;
pub mod uploaded;

use std::path::Path;
use std::sync::Arc;

use filmcraft_media::SharedSource;
use filmcraft_media::reader::{FileReader, SharedReader, open_reader_within};
use serde::{Deserialize, Serialize};
use youcraft_api::records::{Failure, Storyboard};
use youcraft_api::routes::{THUMBNAIL_WIDTHS, media_files};
use youcraft_api::time::MediaDuration;

use crate::ladder::Rung;
use crate::pipeline::Timing;

/// The largest file read whole into memory, for formats that are not read piece by piece.
const MAX_WHOLE_FILE: u64 = 256 << 20;
/// The most pixels a source picture may hold: 8K (8192×4096, which 7680×4320 fits within).
pub const MAX_SOURCE_PIXELS: u64 = 1 << 25;
/// The widest pixel aspect a source may have, either way: 4:1 (anamorphic lenses squeeze 2:1).
pub const MAX_PIXEL_ASPECT: u64 = 4;
/// The widest picture a source may show, either way: 16:1.
pub const MAX_ASPECT: u64 = 16;

/// Why an upload could not be made into renditions.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The file is not a video FilmCraft reads, or not one the site makes.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// A decoder failed on the file.
    #[error("decode: {0}")]
    Decode(String),
    /// An encoder failed.
    #[error("encode: {0}")]
    Encode(String),
    /// Writing a file failed.
    #[error("i/o: {0}")]
    Io(String),
}

impl Error {
    pub(crate) fn io(path: &Path, error: &std::io::Error) -> Error {
        Error::Io(format!("{}: {error}", path.display()))
    }

    fn mux(error: filmcraft_isobmff::Error) -> Error {
        Error::Encode(format!("mp4: {error}"))
    }

    /// What the creator is told of it.
    pub fn failure(&self) -> Failure {
        match self {
            Error::Unsupported(_) => Failure::Unsupported,
            Error::Decode(_) => Failure::Decode,
            Error::Encode(_) | Error::Io(_) => Failure::Internal,
        }
    }
}

/// One rendition that was made.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rendition {
    /// Its name, the shorter side.
    pub label: u32,
    /// Its size in pixels.
    pub width: u32,
    /// Its size in pixels.
    pub height: u32,
    /// Its variant stream's peak segment bit rate, audio included: `BANDWIDTH`.
    pub bandwidth: u64,
    /// Its variant stream's average segment bit rate, audio included: `AVERAGE-BANDWIDTH`.
    pub average_bandwidth: u64,
    /// Its RFC 6381 codecs, as `avc1.64001f,mp4a.40.2`.
    pub codecs: String,
}

/// What [`transcode`] made.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    /// How long the video lasts.
    pub duration: MediaDuration,
    /// The picture's size as shown, in pixels.
    pub width: u32,
    /// The picture's size as shown, in pixels.
    pub height: u32,
    /// The renditions, lowest first.
    pub renditions: Vec<Rendition>,
}

/// What will be made of a source.
pub struct Plan {
    /// Which frames, at what rate.
    pub timing: Timing,
    /// The picture's size in square pixels.
    pub display: (u32, u32),
    /// The renditions.
    pub rungs: Vec<Rung>,
    /// Frames between storyboard tiles.
    pub tile_every: u64,
    /// Whether there is sound.
    pub audio: bool,
}

/// The file of the rendition named `label`.
pub fn video_file(label: u32) -> String {
    format!("{label}p.mp4")
}

/// The media playlist of the rendition named `label`.
pub fn video_playlist(label: u32) -> String {
    format!("{label}p.m3u8")
}

const AUDIO_FILE: &str = "audio.mp4";
const AUDIO_PLAYLIST: &str = "audio.m3u8";

/// Open the media file at `path`.
pub fn open(path: &Path) -> Result<SharedSource, Error> {
    let reader: SharedReader = Arc::new(FileReader::open(path).map_err(|e| Error::io(path, &e))?);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let readers = filmcraft_codecs::reader_openers();
    open_reader_within(
        &name,
        reader,
        &readers,
        &filmcraft_codecs::openers(),
        MAX_WHOLE_FILE,
    )
    .map_err(|e| Error::Unsupported(e.to_string()))
}

/// The size a `width`×`height` picture of `par` pixel aspect shows at, in square pixels: its
/// width stretched by the pixel aspect. Refused when the picture is larger than
/// [`MAX_SOURCE_PIXELS`], or its pixel aspect or its shape is wider than [`MAX_PIXEL_ASPECT`] or
/// [`MAX_ASPECT`] either way, which no camera makes and which would only make giant frames.
pub fn display_size(width: u32, height: u32, par: (u32, u32)) -> Result<(u32, u32), Error> {
    let (w, h) = (u64::from(width), u64::from(height));
    if w == 0 || h == 0 {
        return Err(Error::Unsupported("the video has no picture".into()));
    }
    if w * h > MAX_SOURCE_PIXELS {
        return Err(Error::Unsupported(format!(
            "a picture of {width}×{height} is larger than 8K"
        )));
    }
    let (par_h, par_v) = (u64::from(par.0), u64::from(par.1));
    let (par_h, par_v) = if par_h == 0 || par_v == 0 {
        (1, 1)
    } else {
        (par_h, par_v)
    };
    if par_h > MAX_PIXEL_ASPECT * par_v || par_v > MAX_PIXEL_ASPECT * par_h {
        return Err(Error::Unsupported(format!(
            "a pixel aspect of {par_h}:{par_v}"
        )));
    }
    let shown = (w * par_h + par_v / 2) / par_v;
    if shown > MAX_ASPECT * h || h > MAX_ASPECT * shown {
        return Err(Error::Unsupported(format!(
            "a picture shown at {shown}×{h}"
        )));
    }
    // both sides are within the source's, four times over at most: they fit
    Ok((
        u32::try_from(shown.max(2)).unwrap_or(u32::MAX),
        height.max(2),
    ))
}

/// What to make of `source`.
pub fn plan(source: &SharedSource) -> Result<Plan, Error> {
    let info = source.info();
    let video = info
        .video
        .as_ref()
        .ok_or_else(|| Error::Unsupported("the file has no video".into()))?;
    let timing = Timing::new(video.frame_rate.num, video.frame_rate.den, info.duration)?;
    let display = display_size(video.width, video.height, video.par)?;
    let fps = (timing.timescale.get(), timing.frame.get());
    Ok(Plan {
        timing,
        display,
        rungs: ladder::ladder(display.0, display.1, fps.0, fps.1),
        tile_every: stills::tile_every(timing.frames, u64::from(fps.0), u64::from(fps.1)),
        audio: info.has_audio(),
    })
}

/// Make `source`, as `plan` says, into the media directory `output`, which must exist and be
/// empty; `progress` hears the work done, in thousandths.
pub fn transcode(
    source: &SharedSource,
    plan: &Plan,
    output: &Path,
    progress: &(dyn Fn(u32) + Sync),
) -> Result<Outcome, Error> {
    let (encoded, still) = pipeline::video(source, plan, output, &|done| progress(done * 9 / 10))?;
    let timing = plan.timing;
    let ticks = still.frames.saturating_mul(u64::from(timing.frame.get()));
    let duration = MediaDuration {
        ticks,
        timescale: timing.timescale,
    };
    let sound = if plan.audio {
        let samples =
            u128::from(ticks) * u128::from(audio::SAMPLE_RATE) / u128::from(timing.timescale.get());
        let samples = u64::try_from(samples).unwrap_or(0);
        Some(audio::audio(source, samples, &output.join(AUDIO_FILE))?)
    } else {
        None
    };
    progress(950);
    let write = |name: &str, bytes: &[u8]| -> Result<(), Error> {
        let path = output.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, &e))?;
        }
        std::fs::write(&path, bytes).map_err(|e| Error::io(&path, &e))
    };
    if let Some(sound) = &sound {
        write(AUDIO_PLAYLIST, hls::media(sound, AUDIO_FILE).as_bytes())?;
    }
    let mut renditions = Vec::new();
    for (rung, made) in plan.rungs.iter().zip(&encoded) {
        write(
            &video_playlist(rung.label),
            hls::media(&made.track, &video_file(rung.label)).as_bytes(),
        )?;
        let stream = hls::stream(
            &made.track,
            &made.codec,
            sound.as_ref().map(|track| (track, audio::CODEC)),
        );
        renditions.push(Rendition {
            label: rung.label,
            width: rung.width,
            height: rung.height,
            bandwidth: stream.bandwidth,
            average_bandwidth: stream.average_bandwidth,
            codecs: stream.codecs,
        });
    }
    write(
        media_files::RENDITIONS,
        hls::master(
            &renditions,
            (timing.timescale, timing.frame),
            sound.is_some().then_some(AUDIO_PLAYLIST),
        )
        .as_bytes(),
    )?;
    if let Some(poster) = &still.poster {
        for width in THUMBNAIL_WIDTHS {
            write(
                &media_files::thumbnail(width),
                &stills::jpeg(&stills::thumbnail(poster, plan.display, width))?,
            )?;
        }
    }
    let sheets = stills::sheets(&still.tiles);
    for (index, sheet) in (0u32..).zip(&sheets) {
        write(&media_files::storyboard_sheet(index), &stills::jpeg(sheet)?)?;
    }
    let storyboard = Storyboard {
        interval: MediaDuration {
            ticks: plan
                .tile_every
                .saturating_mul(u64::from(timing.frame.get())),
            timescale: timing.timescale,
        },
        tile_width: stills::TILE.0,
        tile_height: stills::TILE.1,
        columns: stills::COLUMNS,
        rows: stills::ROWS,
        sheets: u32::try_from(sheets.len()).unwrap_or(0),
    };
    let json = serde_json::to_vec(&storyboard).map_err(|e| Error::Encode(e.to_string()))?;
    write(media_files::STORYBOARD, &json)?;
    progress(1000);
    Ok(Outcome {
        duration,
        width: plan.display.0,
        height: plan.display.1,
        renditions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_sizes_apply_the_pixel_aspect_within_bounds() {
        assert_eq!(display_size(1920, 1080, (1, 1)).unwrap(), (1920, 1080));
        // anamorphic 2:1 and PAL 16:9's 64:45
        assert_eq!(display_size(960, 1080, (2, 1)).unwrap(), (1920, 1080));
        assert_eq!(display_size(720, 576, (64, 45)).unwrap(), (1024, 576));
        // a missing pixel aspect is square
        assert_eq!(display_size(640, 360, (0, 0)).unwrap(), (640, 360));
        // a crafted pixel aspect would make a picture 115 million pixels wide
        assert_eq!(
            display_size(1920, 1080, (60_000, 1)).unwrap_err().failure(),
            Failure::Unsupported
        );
        assert!(display_size(1920, 1080, (1, 5)).is_err());
        // 32:9 passes, 20:1 does not, either way
        assert!(display_size(3840, 1080, (1, 1)).is_ok());
        assert!(display_size(21_600, 1080, (1, 1)).is_err());
        assert!(display_size(1080, 21_600, (1, 1)).is_err());
        // 8K passes, more does not
        assert!(display_size(7680, 4320, (1, 1)).is_ok());
        assert!(display_size(8192, 8192, (1, 1)).is_err());
        assert!(display_size(0, 1080, (1, 1)).is_err());
    }
}
