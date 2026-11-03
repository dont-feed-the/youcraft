//! Uploads made into what the site serves: H.264 renditions and an AAC track as CMAF files,
//! their HLS playlists, thumbnails and a storyboard, through FilmCraft's codecs.
//!
//! [`transcode`] reads any file FilmCraft opens (MP4, MOV, Matroska, WebM, MXF, AVI, MPEG-TS…;
//! H.264, HEVC, VP8, VP9, AV1, ProRes…) and writes a video's media directory: the files named
//! by `youcraft_api::routes::media_files` and the track files they refer to. The server runs it
//! in a child process with resource limits, so a file that breaks a decoder breaks nothing else.
//! [`synthetic`] makes videos with known frames and tones for the demo and the tests.

pub mod audio;
pub mod cmaf;
pub mod hls;
pub mod ladder;
pub mod picture;
pub mod pipeline;
pub mod stills;
pub mod synthetic;

use std::path::Path;
use std::sync::Arc;

use filmcraft_media::SharedSource;
use filmcraft_media::reader::{FileReader, SharedReader, open_reader_within};
use serde::{Deserialize, Serialize};
use youcraft_api::records::Storyboard;
use youcraft_api::routes::{THUMBNAIL_WIDTHS, media_files};
use youcraft_api::time::MediaDuration;

use crate::ladder::Rung;
use crate::pipeline::Timing;

/// The largest file read whole into memory, for formats that are not read piece by piece.
const MAX_WHOLE_FILE: u64 = 256 << 20;

/// Why an upload could not be made into renditions.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The file is not a video FilmCraft reads.
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
    fn io(path: &Path, error: &std::io::Error) -> Error {
        Error::Io(format!("{}: {error}", path.display()))
    }

    fn mux(error: filmcraft_isobmff::Error) -> Error {
        Error::Encode(format!("mp4: {error}"))
    }

    /// What kind of failure it is, as the studio tells the creator: `unsupported`, `decode` or
    /// `internal`.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Unsupported(_) => "unsupported",
            Error::Decode(_) => "decode",
            Error::Encode(_) | Error::Io(_) => "internal",
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
    /// Bits per second of its busiest segment, audio included.
    pub bandwidth: u64,
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

/// What to make of `source`.
pub fn plan(source: &SharedSource) -> Result<Plan, Error> {
    let info = source.info();
    let video = info
        .video
        .as_ref()
        .ok_or_else(|| Error::Unsupported("the file has no video".into()))?;
    let timing = Timing::new(video.frame_rate.num, video.frame_rate.den, info.duration)?;
    let (par_h, par_v) = (u64::from(video.par.0.max(1)), u64::from(video.par.1.max(1)));
    let shown = u64::from(video.width) * par_h / par_v;
    let display = (
        u32::try_from(shown).unwrap_or(video.width).max(2),
        video.height.max(2),
    );
    let fps = (u64::from(timing.timescale), u64::from(timing.frame));
    let rate = |v: u64| u32::try_from(v).unwrap_or(u32::MAX);
    Ok(Plan {
        timing,
        display,
        rungs: ladder::ladder(display.0, display.1, rate(fps.0), rate(fps.1)),
        tile_every: stills::tile_every(timing.frames, fps.0, fps.1),
        audio: info.has_audio(),
    })
}

/// Make the media file at `input` into the media directory `output`, which must exist and be
/// empty; `progress` hears the work done, in thousandths.
pub fn transcode(
    input: &Path,
    output: &Path,
    progress: &(dyn Fn(u32) + Sync),
) -> Result<Outcome, Error> {
    let source = open(input)?;
    let plan = plan(&source)?;
    let (encoded, still) =
        pipeline::video(&source, &plan, output, &|done| progress(done * 9 / 10))?;
    let timing = plan.timing;
    let ticks = still.frames * u64::from(timing.frame);
    let duration = MediaDuration {
        ticks,
        timescale: timing.timescale,
    };
    let sound = if plan.audio {
        let samples =
            u128::from(ticks) * u128::from(audio::SAMPLE_RATE) / u128::from(timing.timescale);
        let samples = u64::try_from(samples).unwrap_or(0);
        Some(audio::audio(&source, samples, &output.join(AUDIO_FILE))?)
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
    let playlists: Vec<String> = plan
        .rungs
        .iter()
        .map(|r| format!("{}p.m3u8", r.label))
        .collect();
    let mut variants = Vec::new();
    for ((rung, made), playlist) in plan.rungs.iter().zip(&encoded).zip(&playlists) {
        write(
            playlist,
            hls::media(&made.track, &video_file(rung.label)).as_bytes(),
        )?;
        variants.push(hls::Variant {
            playlist,
            width: rung.width,
            height: rung.height,
            frame_rate: (timing.timescale, timing.frame),
            codec: &made.codec,
            track: &made.track,
        });
    }
    let audio_variant = sound.as_ref().map(|track| hls::Audio {
        playlist: AUDIO_PLAYLIST,
        codec: audio::CODEC,
        track,
    });
    if let Some(sound) = &sound {
        write(AUDIO_PLAYLIST, hls::media(sound, AUDIO_FILE).as_bytes())?;
    }
    write(
        media_files::RENDITIONS,
        hls::master(&variants, audio_variant.as_ref()).as_bytes(),
    )?;
    if let Some(poster) = &still.poster {
        for width in THUMBNAIL_WIDTHS {
            write(
                &media_files::thumbnail(width),
                &stills::jpeg(&stills::thumbnail(poster, width))?,
            )?;
        }
    }
    let sheets = stills::sheets(&still.tiles);
    for (index, sheet) in (0u32..).zip(&sheets) {
        write(&media_files::storyboard_sheet(index), &stills::jpeg(sheet)?)?;
    }
    let storyboard = Storyboard {
        interval: MediaDuration {
            ticks: plan.tile_every * u64::from(timing.frame),
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
    let audio_peak = audio_variant.as_ref().map_or(0, |a| a.track.bandwidth().0);
    let renditions = plan
        .rungs
        .iter()
        .zip(&variants)
        .map(|(rung, variant)| Rendition {
            label: rung.label,
            width: rung.width,
            height: rung.height,
            bandwidth: variant.track.bandwidth().0.saturating_add(audio_peak),
            codecs: match &audio_variant {
                Some(a) => format!("{},{}", variant.codec, a.codec),
                None => variant.codec.to_owned(),
            },
        })
        .collect();
    progress(1000);
    Ok(Outcome {
        duration,
        width: plan.display.0,
        height: plan.display.1,
        renditions,
    })
}
