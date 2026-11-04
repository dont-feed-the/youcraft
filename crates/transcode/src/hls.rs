//! HLS playlists over the CMAF track files: one master playlist, one media playlist per track.
//!
//! Each track is one file; its playlist names the init segment (`EXT-X-MAP`) and every media
//! segment by byte range, so the server only ever serves whole files with `Range` requests.
//! Audio is a rendition group of its own, so changing video quality never fetches audio again.
//! Durations are printed from exact tick counts, rounded only past the sixth decimal. Bit rates
//! are RFC 8216's (§4.1): the peak over runs of segments that last half to one and a half target
//! durations, and the average over the whole track, rounded up so that they bound the truth.

use std::fmt::Write;
use std::num::NonZeroU32;

use crate::Rendition;
use crate::cmaf::Track;

/// The audio group's identifier in the master playlist.
const AUDIO_GROUP: &str = "audio";

/// `ticks / timescale` in decimal: exact up to six decimals, at least three.
pub fn seconds(ticks: u64, timescale: NonZeroU32) -> String {
    let timescale = u128::from(timescale.get());
    let micros = (u128::from(ticks) * 1_000_000 + timescale / 2) / timescale;
    let mut text = format!("{}.{:06}", micros / 1_000_000, micros % 1_000_000);
    while text.ends_with('0') && text.len() > text.find('.').map_or(0, |dot| dot + 4) {
        text.pop();
    }
    text
}

/// The `EXT-X-TARGETDURATION` of `track`, in seconds: its longest segment rounded up, at least 1.
pub fn target_duration(track: &Track) -> u64 {
    let longest = track.segments.iter().map(|s| s.duration).max().unwrap_or(0);
    longest.div_ceil(u64::from(track.timescale.get())).max(1)
}

/// Bits per second of `bytes` played over `ticks` of `track`, rounded up; `None` over no time.
fn bit_rate(track: &Track, bytes: u64, ticks: u64) -> Option<u64> {
    let bits = u128::from(bytes) * 8 * u128::from(track.timescale.get());
    let rate = (ticks != 0).then(|| bits.div_ceil(u128::from(ticks)))?;
    Some(u64::try_from(rate).unwrap_or(u64::MAX))
}

/// The average segment bit rate of `track`: all its segments' bits over its whole duration.
pub fn average_bit_rate(track: &Track) -> u64 {
    let bytes = track.segments.iter().map(|s| s.range.length).sum();
    bit_rate(track, bytes, track.duration()).unwrap_or(0)
}

/// The peak segment bit rate of `track`: the highest bit rate of any run of contiguous segments
/// that lasts between 0.5 and 1.5 target durations. A track too short to hold such a run
/// (under half a second) has its average instead, the bit rate of its only run.
pub fn peak_bit_rate(track: &Track) -> u64 {
    let target = target_duration(track).saturating_mul(u64::from(track.timescale.get()));
    let (shortest, longest) = (u128::from(target), 3 * u128::from(target));
    let mut peak = None;
    for first in 0..track.segments.len() {
        let (mut bytes, mut ticks) = (0u64, 0u64);
        for segment in track.segments.iter().skip(first) {
            bytes = bytes.saturating_add(segment.range.length);
            ticks = ticks.saturating_add(segment.duration);
            // twice the run against the bounds, so that half a target duration stays exact
            let twice = 2 * u128::from(ticks);
            if twice > longest {
                break;
            }
            if twice >= shortest {
                peak = peak.max(bit_rate(track, bytes, ticks));
            }
        }
    }
    peak.unwrap_or_else(|| average_bit_rate(track))
}

/// A variant stream's `BANDWIDTH`, `AVERAGE-BANDWIDTH` and `CODECS`: its video track played with
/// the audio track, if there is one.
pub struct Stream {
    /// The sum of the tracks' peak segment bit rates.
    pub bandwidth: u64,
    /// The sum of the tracks' average segment bit rates.
    pub average_bandwidth: u64,
    /// Its RFC 6381 codecs, video first.
    pub codecs: String,
}

/// The variant stream of the video `track` in `codec`, with `audio`'s track and codec.
pub fn stream(track: &Track, codec: &str, audio: Option<(&Track, &str)>) -> Stream {
    let (bandwidth, average_bandwidth) = (peak_bit_rate(track), average_bit_rate(track));
    match audio {
        Some((sound, sound_codec)) => Stream {
            bandwidth: bandwidth.saturating_add(peak_bit_rate(sound)),
            average_bandwidth: average_bandwidth.saturating_add(average_bit_rate(sound)),
            codecs: format!("{codec},{sound_codec}"),
        },
        None => Stream {
            bandwidth,
            average_bandwidth,
            codecs: codec.to_owned(),
        },
    }
}

/// The master playlist: every rendition, lowest first, each in the media playlist
/// [`crate::video_playlist`] names, at `frame_rate` (`num/den` frames per second), with the
/// audio group of the media playlist `audio` when there is one.
pub fn master(
    renditions: &[Rendition],
    frame_rate: (NonZeroU32, NonZeroU32),
    audio: Option<&str>,
) -> String {
    let mut text = String::from("#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-INDEPENDENT-SEGMENTS\n");
    if let Some(playlist) = audio {
        let _ = writeln!(
            text,
            "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"{AUDIO_GROUP}\",NAME=\"{AUDIO_GROUP}\",DEFAULT=YES,AUTOSELECT=YES,CHANNELS=\"2\",URI=\"{playlist}\""
        );
    }
    let (num, den) = (u64::from(frame_rate.0.get()), u64::from(frame_rate.1.get()));
    let millis = (num * 1000 + den / 2) / den;
    let rate = format!("{}.{:03}", millis / 1000, millis % 1000);
    let group = if audio.is_some() {
        format!(",AUDIO=\"{AUDIO_GROUP}\"")
    } else {
        String::new()
    };
    for rendition in renditions {
        let _ = writeln!(
            text,
            "#EXT-X-STREAM-INF:BANDWIDTH={},AVERAGE-BANDWIDTH={},RESOLUTION={}x{},FRAME-RATE={rate},CODECS=\"{}\"{group}\n{}",
            rendition.bandwidth,
            rendition.average_bandwidth,
            rendition.width,
            rendition.height,
            rendition.codecs,
            crate::video_playlist(rendition.label),
        );
    }
    text
}

/// The media playlist of `track`, stored in the file `file`.
pub fn media(track: &Track, file: &str) -> String {
    let mut text = format!(
        "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:{}\n#EXT-X-PLAYLIST-TYPE:VOD\n\
         #EXT-X-INDEPENDENT-SEGMENTS\n#EXT-X-MAP:URI=\"{file}\",BYTERANGE=\"{}@{}\"\n",
        target_duration(track),
        track.init.length,
        track.init.offset
    );
    for segment in &track.segments {
        let _ = writeln!(
            text,
            "#EXTINF:{},\n#EXT-X-BYTERANGE:{}@{}\n{file}",
            seconds(segment.duration, track.timescale),
            segment.range.length,
            segment.range.offset
        );
    }
    text.push_str("#EXT-X-ENDLIST\n");
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmaf::{Range, Segment};

    fn scale(timescale: u32) -> NonZeroU32 {
        NonZeroU32::new(timescale).unwrap()
    }

    /// A track of `timescale` whose segments are `(bytes, ticks)`, laid end to end after an init
    /// segment of 700 bytes.
    fn track(timescale: u32, segments: &[(u64, u64)]) -> Track {
        let mut offset = 700;
        let segments = segments
            .iter()
            .map(|&(length, duration)| {
                let segment = Segment {
                    range: Range { offset, length },
                    duration,
                };
                offset += length;
                segment
            })
            .collect();
        Track {
            timescale: scale(timescale),
            init: Range {
                offset: 0,
                length: 700,
            },
            segments,
        }
    }

    #[test]
    fn seconds_are_exact_to_six_decimals() {
        assert_eq!(seconds(60_060, scale(30_000)), "2.002");
        assert_eq!(seconds(90_000, scale(90_000)), "1.000");
        assert_eq!(seconds(1001, scale(30_000)), "0.033367");
        assert_eq!(seconds(1, scale(3)), "0.333333");
        assert_eq!(seconds(30_000, scale(1001)), "29.97003");
    }

    #[test]
    fn media_playlists_address_one_file_by_byte_range() {
        let text = media(&track(30_000, &[(250_250, 60_060); 2]), "360p.mp4");
        assert!(text.contains("#EXT-X-TARGETDURATION:3\n"));
        assert!(text.contains("#EXT-X-MAP:URI=\"360p.mp4\",BYTERANGE=\"700@0\"\n"));
        assert!(text.contains("#EXTINF:2.002,\n#EXT-X-BYTERANGE:250250@250950\n360p.mp4\n"));
        assert!(text.ends_with("#EXT-X-ENDLIST\n"));
    }

    #[test]
    fn a_short_last_segment_does_not_set_the_peak() {
        // two segments of 2.002 s at 1 Mbit/s, then an IDR frame alone: 100 kB in 1/30 s
        let video = track(
            30_000,
            &[(250_250, 60_060), (250_250, 60_060), (100_000, 1001)],
        );
        assert_eq!(target_duration(&video), 3);
        // the busiest run of 1.5 to 4.5 s is the last two segments: 350 250 bytes in 61 061 ticks
        assert_eq!(
            peak_bit_rate(&video),
            (350_250u64 * 8 * 30_000).div_ceil(61_061)
        );
        assert!(peak_bit_rate(&video) < 1_400_000);
        assert_eq!(
            average_bit_rate(&video),
            (600_500u64 * 8 * 30_000).div_ceil(121_121)
        );
    }

    #[test]
    fn the_peak_is_the_busiest_run_of_a_target_duration() {
        // a quiet track with one busy segment in the middle
        let video = track(1000, &[(125_000, 2000), (500_000, 2000), (125_000, 2000)]);
        assert_eq!(target_duration(&video), 2);
        assert_eq!(peak_bit_rate(&video), 2_000_000);
        // a video shorter than half its target duration has its only run: the whole
        let tiny = track(1000, &[(1000, 400)]);
        assert_eq!(target_duration(&tiny), 1);
        assert_eq!(peak_bit_rate(&tiny), 20_000);
        assert_eq!(peak_bit_rate(&track(1000, &[])), 0);
    }

    #[test]
    fn the_master_playlist_adds_audio_to_every_variant() {
        let video = track(30_000, &[(250_250, 60_060); 2]);
        let audio = track(48_000, &[(250_250, 60_060); 2]);
        let stream = stream(&video, "avc1.64001e", Some((&audio, "mp4a.40.2")));
        // 250 250 bytes per 2.002 s is 1 000 000 bit/s; the same bytes per 1.25125 s of audio, 1.6 Mbit/s
        assert_eq!(stream.bandwidth, 2_600_000);
        assert_eq!(stream.average_bandwidth, 2_600_000);
        let rendition = Rendition {
            label: 360,
            width: 640,
            height: 360,
            bandwidth: stream.bandwidth,
            average_bandwidth: stream.average_bandwidth,
            codecs: stream.codecs,
        };
        let text = master(
            &[rendition],
            (scale(30_000), scale(1001)),
            Some("audio.m3u8"),
        );
        assert!(text.contains(
            "BANDWIDTH=2600000,AVERAGE-BANDWIDTH=2600000,RESOLUTION=640x360,FRAME-RATE=29.970,\
             CODECS=\"avc1.64001e,mp4a.40.2\",AUDIO=\"audio\"\n360p.m3u8\n"
        ));
        assert!(text.contains("CHANNELS=\"2\",URI=\"audio.m3u8\""));
    }
}
