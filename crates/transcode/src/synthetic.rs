//! Videos generated in code, with known frames and tones: the demo's media and the tests' input.
//!
//! Frame `k` is a smooth gradient tinted by the video's hue, a light square that moves with `k`,
//! and a strip of 16 blocks along the top that spells `k` in binary, so a decoded frame tells
//! which frame it is. The sound is a sine tone on each channel, every channel at its own
//! frequency, so a decoded mix tells where each went. [`write_mp4`] encodes them as H.264 and AAC
//! in an MP4 file, as a camera or an editor would hand them over.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use filmcraft_h264enc::{EncoderConfig, Preset, RateControl};
use filmcraft_isobmff::{Brand, Mp4Writer, TrackConfig, WriteSample, WriterOptions};

use crate::Error;
use crate::picture::Rgba;
use crate::pipeline::{H264, h264, planes};

/// The audio sample rate of generated videos.
pub const SAMPLE_RATE: u32 = crate::audio::SAMPLE_RATE;
/// Blocks in the strip that spells the frame number.
pub const CODE_BITS: u32 = 16;

/// What to generate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Synthetic {
    /// The picture's size in pixels, even.
    pub width: u32,
    /// The picture's size in pixels, even.
    pub height: u32,
    /// The pixel aspect its stream declares, `horizontal:vertical`; `(1, 1)` is square.
    pub par: (u16, u16),
    /// Frames per second, `num/den`.
    pub fps: (u32, u32),
    /// How many frames.
    pub frames: u32,
    /// Its sound; none when `None`.
    pub sound: Option<Tone>,
    /// The tint of the picture, an angle on the colour wheel in 256ths of a turn.
    pub hue: u8,
}

/// A sine tone on every channel: channel `c`, in WAV order, at `(c + 1) × hz` hertz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tone {
    /// The first channel's frequency, in hertz.
    pub hz: u32,
    /// How many channels: 1 (mono), 2 (stereo) or 6 (5.1: L R C LFE Ls Rs).
    pub channels: u16,
}

impl Tone {
    /// The stereo tone at `hz`.
    pub const fn stereo(hz: u32) -> Tone {
        Tone { hz, channels: 2 }
    }

    /// The frequency of `channel`, in hertz.
    pub fn hz_of(&self, channel: u16) -> u32 {
        self.hz.saturating_mul(u32::from(channel) + 1)
    }

    /// Sample `n` of `channel`, between -1 and 1.
    pub fn sample(&self, channel: u16, n: u64) -> f32 {
        let phase = (n as f64 * f64::from(self.hz_of(channel)) / f64::from(SAMPLE_RATE)).fract();
        (0.25 * (phase * std::f64::consts::TAU).sin()) as f32
    }

    /// The WAV-order channel the AAC encoder's channel `element` carries: AAC lists a centre
    /// first, then the pairs, then the LFE.
    fn wav_channel(self, element: u16) -> Result<u16, Error> {
        match (self.channels, element) {
            (1 | 2, element) => Ok(element),
            (6, element) => [2, 0, 1, 4, 5, 3]
                .get(usize::from(element))
                .copied()
                .ok_or_else(|| Error::Unsupported("channel".into())),
            _ => Err(Error::Unsupported(format!("{} channels", self.channels))),
        }
    }
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
    config.sar = spec.par;
    let H264 {
        mut encoder,
        mut entry,
        ..
    } = h264(config)?;
    // the container says it, as cameras write it: FilmCraft reads the pixel aspect from `pasp`
    if let Some(video) = entry.video.as_mut().filter(|_| spec.par != (1, 1)) {
        video.pixel_aspect = Some((u32::from(spec.par.0), u32::from(spec.par.1)));
    }
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
        let packets = encoder
            .encode(&planes(&yuv), i64::from(k))
            .map_err(|e| Error::Encode(e.to_string()))?;
        write(&mut mp4, packets)?;
    }
    write(&mut mp4, encoder.flush())?;
    if let Some(tone) = spec.sound {
        let bitrate = 64_000 * u32::from(tone.channels);
        let (mut aac, track) = crate::audio::aac(tone.channels, bitrate)?;
        let audio = mp4.add_track(track).map_err(Error::mux)?;
        let samples = u64::from(spec.frames) * u64::from(den) * u64::from(SAMPLE_RATE)
            / u64::from(num.max(1));
        let order = (0..tone.channels)
            .map(|element| tone.wav_channel(element))
            .collect::<Result<Vec<_>, _>>()?;
        let mut units = Vec::new();
        let mut start = 0;
        while start < samples {
            let n = (samples - start).min(u64::from(SAMPLE_RATE));
            let channels: Vec<Vec<f32>> = order
                .iter()
                .map(|&channel| {
                    (start..start + n)
                        .map(|i| tone.sample(channel, i))
                        .collect()
                })
                .collect();
            let channels: Vec<&[f32]> = channels.iter().map(Vec::as_slice).collect();
            units.extend(aac.encode(&channels));
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
