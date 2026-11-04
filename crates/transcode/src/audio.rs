//! The sound of an upload as one AAC-LC track: 48 kHz stereo, cut into segments of about two
//! seconds.
//!
//! FilmCraft's source resamples to 48 kHz as it reads, and hands over its channels in WAV order.
//! A source of more than two channels is mixed down as ITU-R BS.775 does: the centre and the
//! surrounds join the front pair 3 dB down and the LFE is left out, each side scaled so that its
//! gains sum to one and it can never clip (see [`downmix`]). The encoder's priming samples are
//! skipped by the init segment's edit list, so the first sample heard is the source's first. The
//! track holds the video's duration of sound, rounded up to whole AAC frames: at most 1023
//! samples (21 ms) more, as a fragmented edit list runs to the end of the track.

use std::path::Path;

use filmcraft_aac::{Encoder, EncoderConfig};
use filmcraft_frame::AudioBuffer;
use filmcraft_isobmff::{SampleEntry, TrackConfig};
use filmcraft_media::SharedSource;

use crate::Error;
use crate::cmaf::{Track, TrackWriter};

/// The audio sample rate.
pub const SAMPLE_RATE: u32 = 48_000;
/// The audio bitrate of a stereo track, in bits per second.
pub const BITRATE: u32 = 128_000;
/// The RFC 6381 codec of AAC-LC.
pub const CODEC: &str = "mp4a.40.2";
/// Samples of one AAC access unit.
const FRAME: u32 = 1024;
/// Access units per segment: 94 × 1024 samples is 2.005 s.
const UNITS_PER_SEGMENT: usize = 94;
/// Samples read at once.
const CHUNK: u64 = 48_000;

/// BS.775's gain of a centre or surround channel in the downmix: −3 dB.
const SIDE: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// An AAC-LC encoder of `channels` channels at `bitrate` bits per second, and the track that
/// holds what it makes, its priming samples skipped by an edit list.
pub fn aac(channels: u16, bitrate: u32) -> Result<(Encoder, TrackConfig), Error> {
    let encoder = Encoder::new(EncoderConfig::cbr(
        SAMPLE_RATE,
        usize::from(channels),
        bitrate,
    ))
    .map_err(|e| Error::Encode(e.to_string()))?;
    let entry = SampleEntry::aac(
        encoder.audio_specific_config(),
        u32::from(channels),
        SAMPLE_RATE,
    );
    let mut config = TrackConfig::new(entry, SAMPLE_RATE);
    config.media_start = Some(i64::from(encoder.priming_samples()));
    Ok((encoder, config))
}

/// How much of each channel of a source of `channels` channels, in WAV order, goes to the left
/// and to the right.
///
/// A count names its usual layout, as WAV and FFmpeg read an unlabelled one: mono; stereo;
/// L R C; quad (L R Ls Rs); 5.0 (L R C Ls Rs); 5.1 (L R C LFE Ls Rs); 6.1 (L R C LFE Cs Ls Rs,
/// the back centre shared by both sides); 7.1 (L R C LFE Lb Rb Ls Rs). Each side's gains sum to
/// one, so a mix of signals within ±1 stays within ±1. A layout of more channels is not known:
/// its first two play as left and right.
pub fn downmix_gains(channels: usize) -> Vec<(f32, f32)> {
    let (left, right, centre, lfe, back) = (
        (1.0, 0.0),
        (0.0, 1.0),
        (SIDE, SIDE),
        (0.0, 0.0),
        (SIDE * SIDE, SIDE * SIDE),
    );
    let (left_surround, right_surround) = ((SIDE, 0.0), (0.0, SIDE));
    let gains: Vec<(f32, f32)> = match channels {
        1 => vec![(1.0, 1.0)],
        3 => vec![left, right, centre],
        4 => vec![left, right, left_surround, right_surround],
        5 => vec![left, right, centre, left_surround, right_surround],
        6 => vec![left, right, centre, lfe, left_surround, right_surround],
        7 => vec![
            left,
            right,
            centre,
            lfe,
            back,
            left_surround,
            right_surround,
        ],
        8 => vec![
            left,
            right,
            centre,
            lfe,
            left_surround,
            right_surround,
            left_surround,
            right_surround,
        ],
        _ => vec![left, right],
    };
    let sum = |side: fn(&(f32, f32)) -> f32| gains.iter().map(side).sum::<f32>();
    let (to_left, to_right) = (sum(|g| g.0), sum(|g| g.1));
    gains
        .iter()
        .map(|&(l, r)| (l / to_left, r / to_right))
        .collect()
}

/// `buffer` mixed down to stereo, exactly `samples` samples long: what it lacks is silence.
pub fn downmix(buffer: &AudioBuffer, samples: usize) -> [Vec<f32>; 2] {
    let mut sides = [vec![0.0f32; samples], vec![0.0f32; samples]];
    let [left, right] = &mut sides;
    for (channel, (to_left, to_right)) in buffer
        .channels
        .iter()
        .zip(downmix_gains(buffer.channels.len()))
    {
        for ((l, r), &sample) in left.iter_mut().zip(right.iter_mut()).zip(channel) {
            *l += to_left * sample;
            *r += to_right * sample;
        }
    }
    sides
}

/// Encode `samples` samples of `source`'s sound into `path`.
pub fn audio(source: &SharedSource, samples: u64, path: &Path) -> Result<Track, Error> {
    let (mut encoder, config) = aac(2, BITRATE)?;
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
        let [left, right] = downmix(&buffer, length);
        write(&mut writer, encoder.encode(&[&left, &right]))?;
        start += n;
    }
    write(&mut writer, encoder.flush())?;
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A buffer whose channel `c` holds the constant `levels[c]`, `samples` long.
    fn constant(levels: &[f32], samples: usize) -> AudioBuffer {
        AudioBuffer {
            sample_rate: SAMPLE_RATE,
            channels: levels.iter().map(|&level| vec![level; samples]).collect(),
        }
    }

    #[test]
    fn every_side_sums_to_one() {
        for channels in 1..=10 {
            let gains = downmix_gains(channels);
            let (left, right): (f32, f32) = gains
                .iter()
                .fold((0.0, 0.0), |(l, r), g| (l + g.0, r + g.1));
            assert!(
                (left - 1.0).abs() < 1e-6 && (right - 1.0).abs() < 1e-6,
                "{channels}"
            );
            // a layout of more than 8 channels is not known: its first two play
            assert_eq!(gains.len(), if channels > 8 { 2 } else { channels });
        }
    }

    #[test]
    fn five_one_keeps_the_dialogue_and_drops_the_lfe() {
        // L R C LFE Ls Rs, each speaker its own level
        let [left, right] = downmix(&constant(&[0.1, 0.2, 0.4, 0.9, 0.05, 0.3], 4), 4);
        let norm = 1.0 + 2.0 * SIDE;
        let expected_left = (0.1 + SIDE * 0.4 + SIDE * 0.05) / norm;
        let expected_right = (0.2 + SIDE * 0.4 + SIDE * 0.3) / norm;
        assert!((left[0] - expected_left).abs() < 1e-6, "{}", left[0]);
        assert!((right[3] - expected_right).abs() < 1e-6, "{}", right[3]);
        // at full scale on every speaker, neither side clips
        let [left, right] = downmix(&constant(&[1.0; 6], 1), 1);
        assert!(left[0] <= 1.0 + 1e-6 && right[0] <= 1.0 + 1e-6);
    }

    #[test]
    fn mono_plays_on_both_sides_and_short_reads_are_padded() {
        let [left, right] = downmix(&constant(&[0.5], 3), 5);
        assert_eq!(left, [0.5, 0.5, 0.5, 0.0, 0.0]);
        assert_eq!(left, right);
        let [left, _] = downmix(&constant(&[0.5, -0.5], 8), 5);
        assert_eq!(left.len(), 5);
        let [silent, _] = downmix(
            &AudioBuffer {
                sample_rate: SAMPLE_RATE,
                channels: Vec::new(),
            },
            2,
        );
        assert_eq!(silent, [0.0, 0.0]);
    }
}
