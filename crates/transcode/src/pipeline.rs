//! One upload made into renditions: decode once, then scale and encode every rung in parallel.
//!
//! The decoder thread reads frame after frame at the output rate (every `step`-th source frame,
//! for the smallest whole `step` that brings the rate to at most 60 frames per second), converts
//! each to RGBA once, and hands it to one thread per rung, which resizes it to the rung's size
//! (the pixel aspect applied on the way), encodes it with H.264 and writes it to its CMAF file.
//! Every rung gets an IDR frame every [`SEGMENT_SECONDS`] at the same frame index, and each
//! segment starts at one, so all renditions cut at the same exact tick and the player switches
//! between them seamlessly. Each encoder cuts its pictures into a number of slices fixed by its
//! size, and its threads are a share of the machine's cores by its pixels, so the same upload
//! makes the same bytes on any machine. Audio follows, resampled to 48 kHz stereo and encoded
//! with AAC.

use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

use filmcraft_h264enc::{Encoder, EncoderConfig, PacketFormat, Preset, RateControl, YuvFrame};
use filmcraft_isobmff::{AvcConfig, SampleEntry, TrackConfig};
use filmcraft_media::{FrameRequest, SharedSource};
use filmcraft_time::{TICKS_PER_SECOND, Tick};

use crate::cmaf::{Track, TrackWriter};
use crate::ladder::Rung;
use crate::picture::{Rgba, Yuv420};
use crate::{Error, Plan};

/// The length of a segment, and the distance between keyframes, in seconds.
pub const SEGMENT_SECONDS: u64 = 2;
/// The highest frame rate of a rendition.
pub const MAX_FPS: u64 = 60;
/// Frames each rung's thread may have waiting.
const QUEUE: usize = 2;
/// Macroblock rows per slice: a 1080p picture is cut in 4 slices, 720p in 3, 360p in 2. The
/// encoder works on the slices of a picture in parallel and does not deblock across their edges.
const SLICE_ROWS: u32 = 17;

/// The frames to make: which source frames, and at what exact rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// Source frames per second, `num/den`, reduced.
    pub source: (u64, u64),
    /// Every `step`-th source frame is kept.
    pub step: u64,
    /// Output ticks per second.
    pub timescale: NonZeroU32,
    /// One output frame, in ticks.
    pub frame: NonZeroU32,
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
        let (out_num, out_den) = (num, den.saturating_mul(step));
        let divisor = gcd(out_num, out_den);
        let (out_num, out_den) = (out_num / divisor, out_den / divisor);
        let multiple = (90_000 / out_num).max(1);
        let rate = |ticks: u64| {
            u32::try_from(ticks)
                .ok()
                .and_then(NonZeroU32::new)
                .ok_or_else(|| Error::Unsupported("frame rate".into()))
        };
        let (timescale, frame) = (
            rate(out_num * multiple)?,
            rate(out_den.saturating_mul(multiple))?,
        );
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

    /// How long the video lasts, in whole seconds, rounded up.
    pub fn seconds(&self) -> u64 {
        u128::from(self.frames)
            .saturating_mul(u128::from(self.frame.get()))
            .div_ceil(u128::from(self.timescale.get()))
            .try_into()
            .unwrap_or(u64::MAX)
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

/// An H.264 encoder, with the sample entry of the track it fills and its RFC 6381 codec.
pub struct H264 {
    /// The encoder.
    pub encoder: Encoder,
    /// What the track's sample description says of its stream.
    pub entry: SampleEntry,
    /// Its codec, as `avc1.64001f`.
    pub codec: String,
}

/// The H.264 encoder `config` describes, writing length-prefixed NAL units as MP4 holds them.
pub fn h264(mut config: EncoderConfig) -> Result<H264, Error> {
    config.format = PacketFormat::LengthPrefixed;
    let (width, height) = (config.width, config.height);
    let encoder = Encoder::new(config).map_err(|e| Error::Encode(e.to_string()))?;
    let avc = AvcConfig::parse(&encoder.avcc()).map_err(Error::mux)?;
    let codec = format!(
        "avc1.{:02x}{:02x}{:02x}",
        avc.profile, avc.compatibility, avc.level
    );
    let side = |side: u32| u16::try_from(side).map_err(|_| Error::Unsupported("too large".into()));
    let entry = SampleEntry::avc(avc, side(width)?, side(height)?);
    Ok(H264 {
        encoder,
        entry,
        codec,
    })
}

/// The planes of `yuv` as the encoder reads them.
pub fn planes(yuv: &Yuv420) -> YuvFrame<'_> {
    YuvFrame {
        y: &yuv.y,
        u: &yuv.u,
        v: &yuv.v,
        y_stride: yuv.width as usize,
        uv_stride: yuv.width as usize / 2,
    }
}

/// The slices of a picture `height` pixels tall: one per [`SLICE_ROWS`] macroblock rows.
fn slices(height: u32) -> usize {
    let rows = height.div_ceil(16);
    usize::try_from(rows.div_ceil(SLICE_ROWS).max(1)).unwrap_or(1)
}

/// The threads of each rung's encoder: the machine's cores shared by pixels, at least one each.
fn threads(rungs: &[Rung]) -> Vec<usize> {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get) as u64;
    let pixels = |rung: &Rung| u64::from(rung.width) * u64::from(rung.height);
    let total = rungs.iter().map(pixels).sum::<u64>().max(1);
    rungs
        .iter()
        .map(|rung| {
            usize::try_from((cores * pixels(rung) + total / 2) / total)
                .unwrap_or(1)
                .max(1)
        })
        .collect()
}

/// What one rung's thread made.
pub struct Encoded {
    /// Its track file.
    pub track: Track,
    /// Its RFC 6381 codec.
    pub codec: String,
}

/// Encode the frames that arrive on `frames` at `rung`'s size into `path`, on `threads` threads.
fn encode_rung(
    rung: Rung,
    timing: Timing,
    threads: usize,
    path: &Path,
    frames: Receiver<Arc<Rgba>>,
) -> Result<Encoded, Error> {
    let mut config = EncoderConfig::new(
        rung.width,
        rung.height,
        timing.timescale.get(),
        timing.frame.get(),
    );
    config.rate = RateControl::Vbr {
        target_kbps: rung.target_kbps,
        max_kbps: rung.max_kbps,
    };
    config.preset = Preset::Speed;
    config.bframes = 0;
    config.keyint = timing.keyint;
    config.scenecut = false;
    config.threads = threads;
    config.slices = slices(rung.height);
    let H264 {
        mut encoder,
        entry,
        codec,
    } = h264(config)?;
    let mut writer = TrackWriter::create(path, TrackConfig::new(entry, timing.timescale.get()))?;
    let write =
        |writer: &mut TrackWriter, packets: Vec<filmcraft_h264enc::Packet>| -> Result<(), Error> {
            for packet in packets {
                if packet.keyframe {
                    writer.cut()?;
                }
                writer.sample(&packet.data, timing.frame.get(), packet.keyframe)?;
            }
            Ok(())
        };
    for (k, picture) in (0i64..).zip(frames) {
        let yuv = if (picture.width, picture.height) == (rung.width, rung.height) {
            picture.to_yuv420()
        } else {
            picture.resized(rung.width, rung.height).to_yuv420()
        };
        let packets = encoder
            .encode(&planes(&yuv), k)
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
    /// The frame thumbnails are made from, at the source's size: a third of the way in, or the
    /// last frame decoded when the file holds fewer.
    pub poster: Option<Arc<Rgba>>,
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
        for (&rung, threads) in plan.rungs.iter().zip(threads(&plan.rungs)) {
            let (sender, receiver) = sync_channel::<Arc<Rgba>>(QUEUE);
            let path = output.join(crate::video_file(rung.label));
            senders.push(sender);
            workers.push(scope.spawn(move || encode_rung(rung, timing, threads, &path, receiver)));
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

/// The decoder thread's loop; it stops early when a rung's thread has stopped, whose error
/// waits at its join.
fn decode(
    source: &SharedSource,
    plan: &Plan,
    senders: &[SyncSender<Arc<Rgba>>],
    progress: &(dyn Fn(u32) + Sync),
) -> Result<Stills, Error> {
    let timing = plan.timing;
    let mut stills = Stills::default();
    let poster_at = timing.frames / 3;
    let tile = crate::picture::fit(plan.display, crate::stills::TILE.0, crate::stills::TILE.1);
    let mut last = None;
    for k in 0..timing.frames {
        let frame = match source.video_frame(FrameRequest::full(timing.time_of(k))) {
            Ok(frame) => frame,
            // a container may promise a frame or two more than it holds; the last segment is
            // then shorter, and nothing else changes
            Err(_) if k > 0 && k + u64::from(timing.keyint) >= timing.frames => break,
            Err(e) => return Err(Error::Decode(e.to_string())),
        };
        let picture = Arc::new(Rgba {
            width: frame.width,
            height: frame.height,
            pixels: frame.to_rgba8(),
        });
        if k == poster_at {
            stills.poster = Some(Arc::clone(&picture));
        }
        if k % plan.tile_every == 0 {
            stills.tiles.push(picture.letterboxed(
                tile,
                crate::stills::TILE.0,
                crate::stills::TILE.1,
            ));
        }
        if senders
            .iter()
            .any(|sender| sender.send(Arc::clone(&picture)).is_err())
        {
            break;
        }
        last = Some(picture);
        stills.frames = k + 1;
        progress(u32::try_from((k + 1) * 1000 / timing.frames).unwrap_or(1000));
    }
    if stills.poster.is_none() {
        stills.poster = last;
    }
    Ok(stills)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(s: i64) -> Tick {
        Tick::from_rational(s, 1, 1)
    }

    #[test]
    fn rates_keep_their_exact_ticks() {
        // 29.97: 90 000 ticks per second, 3003 per frame, a keyframe every 60
        let ntsc = Timing::new(30_000, 1001, seconds(10)).unwrap();
        assert_eq!(
            (ntsc.timescale.get(), ntsc.frame.get(), ntsc.step),
            (90_000, 3003, 1)
        );
        assert_eq!((ntsc.frames, ntsc.keyint, ntsc.seconds()), (299, 60, 10));
        // 23.976 film: 72 000 ticks per second, 3003 per frame
        let film = Timing::new(24_000, 1001, seconds(10)).unwrap();
        assert_eq!(
            (film.timescale.get(), film.frame.get(), film.keyint),
            (72_000, 3003, 48)
        );
        // 25 and 50 frames per second keep their rate
        let pal = Timing::new(50, 1, seconds(4)).unwrap();
        assert_eq!(
            (pal.timescale.get(), pal.frame.get(), pal.frames, pal.keyint),
            (90_000, 1800, 200, 100)
        );
    }

    #[test]
    fn rates_above_sixty_keep_every_step_th_frame() {
        // 120 fps keeps every second frame: 60
        let fast = Timing::new(120, 1, seconds(2)).unwrap();
        assert_eq!(
            (
                fast.step,
                fast.timescale.get(),
                fast.frame.get(),
                fast.frames
            ),
            (2, 90_000, 1500, 120)
        );
        // 144 fps keeps every third: 48, not 72
        let gaming = Timing::new(144, 1, seconds(1)).unwrap();
        assert_eq!((gaming.step, gaming.frames, gaming.keyint), (3, 48, 96));
        // 59.94 stays
        assert_eq!(Timing::new(60_000, 1001, seconds(1)).unwrap().step, 1);
        // the middle of the second kept frame of 120 fps is source frame 2's: 5/240 s
        assert_eq!(fast.time_of(1), Tick::from_rational(5, 1, 240));
    }

    #[test]
    fn hostile_rates_are_refused_or_bounded() {
        assert!(Timing::new(0, 1, seconds(1)).is_err());
        assert!(Timing::new(30, 0, seconds(1)).is_err());
        assert!(Timing::new(-30, 1, seconds(1)).is_err());
        assert!(
            Timing::new(30, 1, Tick::from_rational(1, 1, 1000)).is_err(),
            "no whole frame"
        );
        // a time-lapse of a frame a second has its timescale; a frame a day has none that fits
        let slow = Timing::new(1, 1, seconds(30)).unwrap();
        assert_eq!(
            (slow.timescale.get(), slow.frame.get(), slow.keyint),
            (90_000, 90_000, 2)
        );
        assert!(Timing::new(1, 86_400, seconds(86_400 * 3)).is_err());
        // an absurd rate is brought below sixty
        let absurd = Timing::new(i64::from(u32::MAX), 1, seconds(1)).unwrap();
        assert!(absurd.source.0.div_ceil(absurd.source.1 * absurd.step) <= MAX_FPS);
    }

    #[test]
    fn slices_and_threads_depend_on_sizes_only() {
        assert_eq!(
            (slices(1080), slices(720), slices(360), slices(2)),
            (4, 3, 2, 1)
        );
        let rungs = crate::ladder::ladder(1920, 1080, 30, 1);
        let shares = threads(&rungs);
        assert_eq!(shares.len(), 3);
        assert!(shares.iter().all(|&t| t >= 1));
        assert!(shares[2] >= shares[1] && shares[1] >= shares[0]);
    }
}
