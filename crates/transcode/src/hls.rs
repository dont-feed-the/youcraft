//! HLS playlists over the CMAF track files: one master playlist, one media playlist per track.
//!
//! Each track is one file; its playlist names the init segment (`EXT-X-MAP`) and every media
//! segment by byte range, so the server only ever serves whole files with `Range` requests.
//! Audio is a rendition group of its own, so changing video quality never fetches audio again.
//! Durations are printed from exact tick counts, rounded only past the sixth decimal.

use std::fmt::Write;

use crate::cmaf::Track;

/// The audio group's identifier in the master playlist.
const AUDIO_GROUP: &str = "audio";

/// A video rendition as the master playlist lists it.
pub struct Variant<'a> {
    /// Its media playlist, relative to the master playlist.
    pub playlist: &'a str,
    /// Its size in pixels.
    pub width: u32,
    /// Its size in pixels.
    pub height: u32,
    /// Its frame rate, `num/den` frames per second.
    pub frame_rate: (u32, u32),
    /// Its RFC 6381 codec, as `avc1.64001f`.
    pub codec: &'a str,
    /// Its track.
    pub track: &'a Track,
}

/// The audio rendition, as the master playlist lists it.
pub struct Audio<'a> {
    /// Its media playlist, relative to the master playlist.
    pub playlist: &'a str,
    /// Its RFC 6381 codec, as `mp4a.40.2`.
    pub codec: &'a str,
    /// Its track.
    pub track: &'a Track,
}

/// `ticks / timescale` in decimal: exact up to six decimals, at least three.
pub fn seconds(ticks: u64, timescale: u32) -> String {
    let timescale = u128::from(timescale.max(1));
    let micros = (u128::from(ticks) * 1_000_000 + timescale / 2) / timescale;
    let mut text = format!("{}.{:06}", micros / 1_000_000, micros % 1_000_000);
    while text.ends_with('0') && text.len() > text.find('.').map_or(0, |dot| dot + 4) {
        text.pop();
    }
    text
}

/// The master playlist: every variant, lowest first, with the audio group when there is one.
pub fn master(variants: &[Variant<'_>], audio: Option<&Audio<'_>>) -> String {
    let mut text = String::from("#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-INDEPENDENT-SEGMENTS\n");
    if let Some(audio) = audio {
        let _ = writeln!(
            text,
            "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"{AUDIO_GROUP}\",NAME=\"{AUDIO_GROUP}\",DEFAULT=YES,AUTOSELECT=YES,URI=\"{}\"",
            audio.playlist
        );
    }
    let (audio_peak, audio_average) = audio.map_or((0, 0), |a| a.track.bandwidth());
    for variant in variants {
        let (peak, average) = variant.track.bandwidth();
        let (num, den) = variant.frame_rate;
        let millis = (u64::from(num) * 1000 + u64::from(den.max(1)) / 2) / u64::from(den.max(1));
        let rate = format!("{}.{:03}", millis / 1000, millis % 1000);
        let (codecs, group) = match audio {
            Some(a) => (
                format!("{},{}", variant.codec, a.codec),
                format!(",AUDIO=\"{AUDIO_GROUP}\""),
            ),
            None => (variant.codec.to_owned(), String::new()),
        };
        let _ = writeln!(
            text,
            "#EXT-X-STREAM-INF:BANDWIDTH={},AVERAGE-BANDWIDTH={},RESOLUTION={}x{},FRAME-RATE={rate},CODECS=\"{codecs}\"{group}\n{}",
            peak.saturating_add(audio_peak),
            average.saturating_add(audio_average),
            variant.width,
            variant.height,
            variant.playlist,
        );
    }
    text
}

/// The media playlist of `track`, stored in the file `file`.
pub fn media(track: &Track, file: &str) -> String {
    let longest = track.segments.iter().map(|s| s.duration).max().unwrap_or(0);
    let target = longest.div_ceil(u64::from(track.timescale.max(1))).max(1);
    let mut text = format!(
        "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:{target}\n#EXT-X-PLAYLIST-TYPE:VOD\n\
         #EXT-X-INDEPENDENT-SEGMENTS\n#EXT-X-MAP:URI=\"{file}\",BYTERANGE=\"{}@{}\"\n",
        track.init.length, track.init.offset
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

    fn track() -> Track {
        let segment = |offset, start| Segment {
            range: Range {
                offset,
                length: 250_250,
            },
            start,
            duration: 60_060,
        };
        Track {
            timescale: 30_000,
            init: Range {
                offset: 0,
                length: 700,
            },
            segments: vec![segment(700, 0), segment(250_950, 60_060)],
        }
    }

    #[test]
    fn seconds_are_exact_to_six_decimals() {
        assert_eq!(seconds(60_060, 30_000), "2.002");
        assert_eq!(seconds(90_000, 90_000), "1.000");
        assert_eq!(seconds(1001, 30_000), "0.033367");
        assert_eq!(seconds(1, 3), "0.333333");
        assert_eq!(seconds(30_000, 1001), "29.97003");
    }

    #[test]
    fn media_playlists_address_one_file_by_byte_range() {
        let text = media(&track(), "360p.mp4");
        assert!(text.contains("#EXT-X-TARGETDURATION:3\n"));
        assert!(text.contains("#EXT-X-MAP:URI=\"360p.mp4\",BYTERANGE=\"700@0\"\n"));
        assert!(text.contains("#EXTINF:2.002,\n#EXT-X-BYTERANGE:250250@250950\n360p.mp4\n"));
        assert!(text.ends_with("#EXT-X-ENDLIST\n"));
    }

    #[test]
    fn the_master_playlist_adds_audio_to_every_variant() {
        let video = track();
        let audio_track = Track {
            timescale: 48_000,
            ..track()
        };
        let variant = Variant {
            playlist: "360p.m3u8",
            width: 640,
            height: 360,
            frame_rate: (30_000, 1001),
            codec: "avc1.64001e",
            track: &video,
        };
        let audio = Audio {
            playlist: "audio.m3u8",
            codec: "mp4a.40.2",
            track: &audio_track,
        };
        let text = master(&[variant], Some(&audio));
        // 250 250 bytes per 2.002 s is 1 000 000 bit/s; the same bytes per 1.25125 s of audio, 1.6 Mbit/s
        assert!(text.contains(
            "BANDWIDTH=2600000,AVERAGE-BANDWIDTH=2600000,RESOLUTION=640x360,FRAME-RATE=29.970,\
             CODECS=\"avc1.64001e,mp4a.40.2\",AUDIO=\"audio\"\n360p.m3u8\n"
        ));
        assert!(text.contains("URI=\"audio.m3u8\""));
    }
}
