//! One upload made into renditions: decode once, then scale and encode every rung in parallel.
//!
//! The decoder thread reads frame after frame at the output rate (the source's, halved until
//! it is at most 60 frames per second), converts each to RGBA once, and hands it to one thread
//! per rung, which resizes it, encodes it with H.264 and writes it to its CMAF file. Every rung
//! gets an IDR frame every [`SEGMENT_SECONDS`] at the same frame index, and each segment starts
//! at one, so all renditions cut at the same exact tick and the player switches between them
//! seamlessly. Audio follows, resampled to 48 kHz stereo and encoded with AAC.

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, sync_channel};

use filmcraft_h264enc::{Encoder, EncoderConfig, PacketFormat, Preset, RateControl, YuvFrame};
use filmcraft_isobmff::{AvcConfig, SampleEntry, TrackConfig};
use filmcraft_media::{FrameRequest, SharedSource};
use filmcraft_time::{TICKS_PER_SECOND, Tick};

use crate::cmaf::{Track, TrackWriter};
use crate::ladder::Rung;
use crate::picture::Rgba;
use crate::{Error, Plan};

/// The length of a segment, and the distance between keyframes, in seconds.
pub const SEGMENT_SECONDS: u64 = 2;
/// The highest frame rate of a rendition.
pub const MAX_FPS: u64 = 60;
/// Frames each rung's thread may have waiting.
const QUEUE: usize = 2;

/// The frames to make: which source frames, and at what exact rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// Source frames per second, `num/den`, reduced.
    pub source: (u64, u64),
    /// Every `step`-th source frame is kept.
    pub step: u64,
    /// Output ticks per second.
    pub timescale: u32,
    /// One output frame, in ticks.
    pub frame: u32,
    /// Output frames.
    pub frames: u64,
    /// Output frames per segment.
    pub keyint: u32,
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

impl Timing {
    /// The timing for a source at `num/den` frames per second that lasts `duration`.
    pub fn new(num: i64, den: i64, duration: Tick) -> Result<Timing, Error> {
        let (num, den) = (
            u64::try_from(num).unwrap_or(0),
            u64::try_from(den).unwrap_or(0),
        );
        if num == 0 || den == 0 {
            return Err(Error::Unsupported("the video has no frame rate".into()));
        }
        let divisor = gcd(num, den);
        let (num, den) = (num / divisor, den / divisor);
        let step = (num.div_ceil(den)).div_ceil(MAX_FPS).max(1);
        let (out_num, out_den) = (num, den * step);
        let divisor = gcd(out_num, out_den);
        let (out_num, out_den) = (out_num / divisor, out_den / divisor);
        let multiple = (90_000 / out_num).max(1);
        let timescale = u32::try_from(out_num * multiple)
            .map_err(|_| Error::Unsupported("frame rate".into()))?;
        let frame = u32::try_from(out_den * multiple)
            .map_err(|_| Error::Unsupported("frame rate".into()))?;
        let ticks = u128::try_from(duration.0).unwrap_or(0);
        let frames = ticks * u128::from(out_num) / (u128::from(out_den) * TICKS_PER_SECOND as u128);
        let frames = u64::try_from(frames).unwrap_or(u64::MAX);
        if frames == 0 {
            return Err(Error::Unsupported("the video has no frames".into()));
        }
        let keyint = (SEGMENT_SECONDS * out_num + out_den / 2) / out_den;
        let keyint = u32::try_from(keyint.max(1)).unwrap_or(u32::MAX);
        Ok(Timing {
            source: (num, den),
            step,
            timescale,
            frame,
            frames,
            keyint,
        })
    }

    /// The source time to decode output frame `k` at: the middle of its source frame, so that
    /// a timestamp rounded either way still finds it.
    pub fn time_of(&self, k: u64) -> Tick {
        let (num, den) = self.source;
        let index = i64::try_from(k.saturating_mul(self.step)).unwrap_or(i64::MAX / 4);
        let den = i64::try_from(den).unwrap_or(1);
        let num = i64::try_from(num).unwrap_or(1);
        Tick::from_rational(
            index.saturating_mul(2).saturating_add(1),
            den,
            num.saturating_mul(2),
        )
    }
}

/// What one rung's thread made.
pub struct Encoded {
    /// Its track file.
    pub track: Track,
    /// Its RFC 6381 codec.
    pub codec: String,
}

/// Encode the frames that arrive on `frames` at `rung`'s size into `path`.
fn encode_rung(
    rung: Rung,
    timing: Timing,
    path: &Path,
    frames: Receiver<Arc<Rgba>>,
) -> Result<Encoded, Error> {
    let mut config = EncoderConfig::new(rung.width, rung.height, timing.timescale, timing.frame);
    config.rate = RateControl::Vbr {
        target_kbps: rung.target_kbps,
        max_kbps: rung.max_kbps,
    };
    config.preset = Preset::Speed;
    config.bframes = 0;
    config.keyint = timing.keyint;
    config.scenecut = false;
    config.format = PacketFormat::LengthPrefixed;
    let mut encoder = Encoder::new(config).map_err(|e| Error::Encode(e.to_string()))?;
    let avc = AvcConfig::parse(&encoder.avcc()).map_err(Error::mux)?;
    let codec = format!(
        "avc1.{:02x}{:02x}{:02x}",
        avc.profile, avc.compatibility, avc.level
    );
    let size = |side: u32| u16::try_from(side).map_err(|_| Error::Unsupported("too large".into()));
    let entry = SampleEntry::avc(avc, size(rung.width)?, size(rung.height)?);
    let mut writer = TrackWriter::create(path, TrackConfig::new(entry, timing.timescale))?;
    let write =
        |writer: &mut TrackWriter, packets: Vec<filmcraft_h264enc::Packet>| -> Result<(), Error> {
            for packet in packets {
                if packet.keyframe {
                    writer.cut()?;
                }
                writer.sample(&packet.data, timing.frame, packet.keyframe)?;
            }
            Ok(())
        };
    for (k, picture) in (0i64..).zip(frames) {
        let yuv = picture.resized(rung.width, rung.height).to_yuv420();
        let (y_stride, uv_stride) = (rung.width as usize, rung.width as usize / 2);
        let input = YuvFrame {
            y: &yuv.y,
            u: &yuv.u,
            v: &yuv.v,
            y_stride,
            uv_stride,
        };
        let packets = encoder
            .encode(&input, k)
            .map_err(|e| Error::Encode(e.to_string()))?;
        write(&mut writer, packets)?;
    }
    write(&mut writer, encoder.flush())?;
    Ok(Encoded {
        track: writer.finish()?,
        codec,
    })
}

/// What the decoder thread keeps besides the renditions.
#[derive(Default)]
pub struct Stills {
    /// The frame thumbnails are made from.
    pub poster: Option<Rgba>,
    /// The storyboard's tiles.
    pub tiles: Vec<Rgba>,
    /// Frames decoded, which may be fewer than planned when the last ones are missing.
    pub frames: u64,
}

/// Decode `timing.frames` frames of `source` and encode them at every rung of `plan`, into
/// `output`; `progress` hears each frame done, in thousandths of the video.
pub fn video(
    source: &SharedSource,
    plan: &Plan,
    output: &Path,
    progress: &(dyn Fn(u32) + Sync),
) -> Result<(Vec<Encoded>, Stills), Error> {
    let timing = plan.timing;
    std::thread::scope(|scope| {
        let mut senders = Vec::new();
        let mut workers = Vec::new();
        for &rung in &plan.rungs {
            let (sender, receiver) = sync_channel::<Arc<Rgba>>(QUEUE);
            let path = output.join(crate::video_file(rung.label));
            senders.push(sender);
            workers.push(scope.spawn(move || encode_rung(rung, timing, &path, receiver)));
        }
        let stills = decode(source, plan, &senders, progress);
        drop(senders);
        let mut encoded = Vec::new();
        for worker in workers {
            encoded.push(
                worker
                    .join()
                    .map_err(|_| Error::Encode("an encoder stopped".into()))??,
            );
        }
        Ok((encoded, stills?))
    })
}

/// The decoder thread's loop.
fn decode(
    source: &SharedSource,
    plan: &Plan,
    senders: &[std::sync::mpsc::SyncSender<Arc<Rgba>>],
    progress: &(dyn Fn(u32) + Sync),
) -> Result<Stills, Error> {
    let timing = plan.timing;
    let mut stills = Stills::default();
    let poster_at = timing.frames / 3;
    for k in 0..timing.frames {
        let frame = match source.video_frame(FrameRequest::full(timing.time_of(k))) {
            Ok(frame) => frame,
            // a container may promise a frame or two more than it holds; the last segment is
            // then shorter, and nothing else changes
            Err(_) if k > 0 && k + u64::from(timing.keyint) >= timing.frames => break,
            Err(e) => return Err(Error::Decode(e.to_string())),
        };
        let picture = Rgba {
            width: frame.width,
            height: frame.height,
            pixels: frame.to_rgba8(),
        };
        let picture = if (picture.width, picture.height) == plan.display {
            picture
        } else {
            picture.resized(plan.display.0, plan.display.1)
        };
        if k == poster_at {
            stills.poster = Some(picture.clone());
        }
        if k % plan.tile_every == 0 {
            stills
                .tiles
                .push(picture.fitted(crate::stills::TILE.0, crate::stills::TILE.1));
        }
        let picture = Arc::new(picture);
        for sender in senders {
            // a rung that stopped has its error waiting at join
            let _ = sender.send(Arc::clone(&picture));
        }
        stills.frames = k + 1;
        progress(u32::try_from((k + 1) * 1000 / timing.frames).unwrap_or(1000));
    }
    Ok(stills)
}
