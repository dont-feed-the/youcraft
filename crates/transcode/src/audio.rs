//! The sound of an upload as one AAC-LC track: 48 kHz stereo, cut into segments of about two
//! seconds.
//!
//! FilmCraft's source resamples to 48 kHz as it reads. A mono source plays on both sides; a
//! source of more channels keeps its first two, front left and front right in every common
//! layout. The encoder's priming samples are skipped by the init segment's edit list, so the
//! first sample heard is the source's first. The track lasts exactly as long as the video.

use std::path::Path;

use filmcraft_aac::{Encoder, EncoderConfig};
use filmcraft_isobmff::{SampleEntry, TrackConfig};
use filmcraft_media::SharedSource;

use crate::Error;
use crate::cmaf::{Track, TrackWriter};

/// The audio sample rate.
pub const SAMPLE_RATE: u32 = 48_000;
/// The audio bitrate, in bits per second.
pub const BITRATE: u32 = 128_000;
/// The RFC 6381 codec of AAC-LC.
pub const CODEC: &str = "mp4a.40.2";
/// Samples of one AAC access unit.
const FRAME: u32 = 1024;
/// Access units per segment: 94 × 1024 samples is 2.005 s.
const UNITS_PER_SEGMENT: usize = 94;
/// Samples read at once.
const CHUNK: u64 = 48_000;

/// Encode `samples` samples of `source`'s sound into `path`.
pub fn audio(source: &SharedSource, samples: u64, path: &Path) -> Result<Track, Error> {
    let mut encoder = Encoder::new(EncoderConfig::cbr(SAMPLE_RATE, 2, BITRATE))
        .map_err(|e| Error::Encode(e.to_string()))?;
    let entry = SampleEntry::aac(encoder.audio_specific_config(), 2, SAMPLE_RATE);
    let mut config = TrackConfig::new(entry, SAMPLE_RATE);
    config.media_start = Some(i64::from(encoder.priming_samples()));
    let mut writer = TrackWriter::create(path, config)?;
    let mut units = 0usize;
    let mut write = |writer: &mut TrackWriter, access_units: Vec<Vec<u8>>| -> Result<(), Error> {
        for unit in access_units {
            if units > 0 && units.is_multiple_of(UNITS_PER_SEGMENT) {
                writer.cut()?;
            }
            writer.sample(&unit, FRAME, true)?;
            units += 1;
        }
        Ok(())
    };
    let mut start = 0u64;
    while start < samples {
        let n = (samples - start).min(CHUNK);
        let first = i64::try_from(start).unwrap_or(i64::MAX);
        let length = usize::try_from(n).unwrap_or(0);
        let buffer = source
            .audio(first, length, SAMPLE_RATE)
            .map_err(|e| Error::Decode(e.to_string()))?;
        let silence = vec![0.0f32; length];
        let left = buffer
            .channels
            .first()
            .map_or(silence.as_slice(), Vec::as_slice);
        let right = buffer.channels.get(1).map_or(left, Vec::as_slice);
        write(&mut writer, encoder.encode(&[left, right]))?;
        start += n;
    }
    write(&mut writer, encoder.flush())?;
    writer.finish()
}
