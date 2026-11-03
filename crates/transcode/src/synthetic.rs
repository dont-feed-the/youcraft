//! Videos generated in code, with known frames and tones: the demo's media and the tests' input.
//!
//! Frame `k` is a smooth gradient tinted by the video's hue, a light square that moves with `k`,
//! and a strip of 16 blocks along the top that spells `k` in binary, so a decoded frame tells
//! which frame it is. The sound is a sine tone. [`write_mp4`] encodes them as H.264 and AAC in
//! an MP4 file, as a camera or an editor would hand them over.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use filmcraft_aac::{Encoder as AacEncoder, EncoderConfig as AacConfig};
use filmcraft_h264enc::{Encoder, EncoderConfig, PacketFormat, Preset, RateControl, YuvFrame};
use filmcraft_isobmff::{
    AvcConfig, Brand, Mp4Writer, SampleEntry, TrackConfig, WriteSample, WriterOptions,
};

use crate::Error;
use crate::picture::Rgba;

/// The audio sample rate of generated videos.
pub const SAMPLE_RATE: u32 = 48_000;
/// Blocks in the strip that spells the frame number.
pub const CODE_BITS: u32 = 16;

/// What to generate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Synthetic {
    /// The picture's size in pixels, even.
    pub width: u32,
    /// The picture's size in pixels, even.
    pub height: u32,
    /// Frames per second, `num/den`.
    pub fps: (u32, u32),
    /// How many frames.
    pub frames: u32,
    /// The tone's frequency in hertz; no sound when `None`.
    pub tone_hz: Option<u32>,
    /// The tint of the picture, an angle on the colour wheel in 256ths of a turn.
    pub hue: u8,
}

/// R'G'B' of `hue` (256ths of a turn) at full saturation, between `low` and `high`.
fn tint(hue: u8, low: u32, high: u32) -> [u32; 3] {
    let h = u32::from(hue) * 6;
    let (sector, f) = (h / 256, h % 256);
    let up = low + (high - low) * f / 256;
    let down = high - (high - low) * f / 256;
    match sector {
        0 => [high, up, low],
        1 => [down, high, low],
        2 => [low, high, up],
        3 => [low, down, high],
        4 => [up, low, high],
        _ => [high, low, down],
    }
}

/// Frame `k` of `spec`.
pub fn frame(spec: &Synthetic, k: u32) -> Rgba {
    let (w, h) = (spec.width.max(2), spec.height.max(2));
    let base = tint(spec.hue, 40, 200);
    let mut pixels = Vec::with_capacity(w as usize * h as usize * 4);
    let side = (h / 4).max(2);
    let travel = w.saturating_sub(side).max(1);
    let square_x = (k * 7) % travel;
    let square_y = h / 2 - side / 2;
    let block = (w / CODE_BITS).max(1);
    let strip = (h / 20).max(2);
    for y in 0..h {
        for x in 0..w {
            let in_square = (square_x..square_x + side).contains(&x)
                && (square_y..square_y + side).contains(&y);
            let rgb = if y < strip && x < block * CODE_BITS {
                let bit = (k >> (CODE_BITS - 1 - x / block)) & 1;
                if bit == 1 { [235; 3] } else { [20; 3] }
            } else if in_square {
                [240, 240, 230]
            } else {
                let shade = 60 + 140 * (x + y) / (w + h);
                base.map(|c| c * shade / 200)
            };
            pixels.extend(rgb.map(|c| c.min(255) as u8));
            pixels.push(255);
        }
    }
    Rgba {
        width: w,
        height: h,
        pixels,
    }
}

/// The frame number that the strip of a decoded `luma` plane spells.
pub fn read_code(luma: &[u8], width: u32, height: u32) -> u32 {
    let block = (width / CODE_BITS).max(1);
    let y = (height / 40).max(1) as usize;
    (0..CODE_BITS).fold(0, |code, bit| {
        let x = (bit * block + block / 2) as usize;
        let value = luma.get(y * width as usize + x).copied().unwrap_or(0);
        code << 1 | u32::from(value > 128)
    })
}

/// Sample `n` of the tone, between -1 and 1.
pub fn tone(spec: &Synthetic, n: u64) -> f32 {
    let hz = f64::from(spec.tone_hz.unwrap_or(0));
    let phase = (n as f64 * hz / f64::from(SAMPLE_RATE)).fract();
    (0.25 * (phase * std::f64::consts::TAU).sin()) as f32
}

/// Write `spec` to `path` as an MP4 file.
pub fn write_mp4(spec: &Synthetic, path: &Path) -> Result<(), Error> {
    let file = File::create(path).map_err(|e| Error::io(path, &e))?;
    let mut mp4 =
        Mp4Writer::new(BufWriter::new(file), WriterOptions::new(Brand::Mp4)).map_err(Error::mux)?;
    let (num, den) = spec.fps;
    let mut config = EncoderConfig::new(spec.width, spec.height, num, den);
    config.rate = RateControl::Crf(16.0);
    config.preset = Preset::Speed;
    config.bframes = 0;
    config.format = PacketFormat::LengthPrefixed;
    let mut encoder = Encoder::new(config).map_err(|e| Error::Encode(e.to_string()))?;
    let avc = AvcConfig::parse(&encoder.avcc()).map_err(Error::mux)?;
    let size = |side: u32| u16::try_from(side).map_err(|_| Error::Unsupported("too large".into()));
    let entry = SampleEntry::avc(avc, size(spec.width)?, size(spec.height)?);
    let video = mp4
        .add_track(TrackConfig::new(entry, num))
        .map_err(Error::mux)?;
    let write =
        |mp4: &mut Mp4Writer<_>, packets: Vec<filmcraft_h264enc::Packet>| -> Result<(), Error> {
            for packet in packets {
                let sample = WriteSample {
                    data: &packet.data,
                    duration: den,
                    composition_offset: 0,
                    is_sync: packet.keyframe,
                };
                mp4.write_sample(video, sample).map_err(Error::mux)?;
            }
            Ok(())
        };
    for k in 0..spec.frames {
        let yuv = frame(spec, k).to_yuv420();
        let (w, cw) = (spec.width as usize, spec.width as usize / 2);
        let input = YuvFrame {
            y: &yuv.y,
            u: &yuv.u,
            v: &yuv.v,
            y_stride: w,
            uv_stride: cw,
        };
        let packets = encoder
            .encode(&input, i64::from(k))
            .map_err(|e| Error::Encode(e.to_string()))?;
        write(&mut mp4, packets)?;
    }
    write(&mut mp4, encoder.flush())?;
    if spec.tone_hz.is_some() {
        let mut aac = AacEncoder::new(AacConfig::cbr(SAMPLE_RATE, 2, 128_000))
            .map_err(|e| Error::Encode(e.to_string()))?;
        let mut track = TrackConfig::new(
            SampleEntry::aac(aac.audio_specific_config(), 2, SAMPLE_RATE),
            SAMPLE_RATE,
        );
        track.media_start = Some(i64::from(aac.priming_samples()));
        let audio = mp4.add_track(track).map_err(Error::mux)?;
        let samples = u64::from(spec.frames) * u64::from(den) * u64::from(SAMPLE_RATE)
            / u64::from(num.max(1));
        let mut units = Vec::new();
        let mut start = 0;
        while start < samples {
            let n = (samples - start).min(u64::from(SAMPLE_RATE));
            let mono: Vec<f32> = (start..start + n).map(|i| tone(spec, i)).collect();
            units.extend(aac.encode(&[&mono, &mono]));
            start += n;
        }
        units.extend(aac.flush());
        for unit in units {
            let sample = WriteSample {
                data: &unit,
                duration: 1024,
                composition_offset: 0,
                is_sync: true,
            };
            mp4.write_sample(audio, sample).map_err(Error::mux)?;
        }
    }
    mp4.finish().map_err(Error::mux)?;
    Ok(())
}
