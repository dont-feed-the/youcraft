//! HLS playlists as YouCraft serves them (RFC 8216, version 7): the master playlist that lists a
//! video's renditions, and the media playlist of each, whose segments are byte ranges of one
//! CMAF file.
//!
//! Times are read exactly: an `EXTINF` decimal of up to nine places is a whole number of
//! nanoseconds, and every segment's start is the exact sum of the durations before it. Input is
//! hostile, so every count is bounded and every malformed line is an error naming it.

use std::fmt;

/// Ticks per second of every playlist time: nanoseconds, exact for any decimal of nine places.
pub const NANOS: u64 = 1_000_000_000;

/// The most lines a playlist may have.
const MAX_LINES: usize = 200_000;
/// The most renditions a master playlist may list.
const MAX_VARIANTS: usize = 32;

/// What is wrong with a playlist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaylistError {
    /// It does not start with `#EXTM3U`.
    NotAPlaylist,
    /// Line `line` (counted from 1) cannot be read.
    Malformed {
        /// The line.
        line: usize,
    },
    /// It is longer than any playlist of ours.
    TooLarge,
    /// It lists no rendition, or no segment.
    Empty,
}

impl fmt::Display for PlaylistError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlaylistError::NotAPlaylist => write!(f, "not an HLS playlist"),
            PlaylistError::Malformed { line } => write!(f, "malformed playlist line {line}"),
            PlaylistError::TooLarge => write!(f, "playlist too large"),
            PlaylistError::Empty => write!(f, "playlist lists nothing to play"),
        }
    }
}

/// Bytes `offset .. offset + length` of a resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ByteRange {
    /// The first byte.
    pub offset: u64,
    /// How many bytes; never zero.
    pub length: u64,
}

impl ByteRange {
    /// The value of an HTTP `Range` header asking for these bytes.
    pub fn header(self) -> String {
        let last = self.offset.saturating_add(self.length.saturating_sub(1));
        format!("bytes={}-{last}", self.offset)
    }
}

/// Something to fetch: an address, and the bytes of it, or all of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    /// The address, resolved against the playlist's.
    pub uri: String,
    /// The bytes; `None` for the whole resource.
    pub range: Option<ByteRange>,
}

impl Resource {
    /// One resource for `self` followed by `next`, when they are adjacent bytes of the same
    /// address: one request instead of two.
    pub fn joined(&self, next: &Resource) -> Option<Resource> {
        let (first, second) = (self.range?, next.range?);
        let adjacent =
            self.uri == next.uri && first.offset.checked_add(first.length)? == second.offset;
        adjacent.then(|| Resource {
            uri: self.uri.clone(),
            range: Some(ByteRange {
                offset: first.offset,
                length: first.length.saturating_add(second.length),
            }),
        })
    }
}

/// One rendition of a master playlist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    /// Its peak bit rate, in bits per second.
    pub bandwidth: u64,
    /// Its width in pixels; 0 if the playlist does not say.
    pub width: u32,
    /// Its height in pixels; 0 if the playlist does not say.
    pub height: u32,
    /// Its codecs, as `CODECS` lists them: `avc1.64001f,mp4a.40.2`.
    pub codecs: String,
    /// Its media playlist.
    pub uri: String,
}

impl Variant {
    /// The codecs of its pictures, as a MIME `codecs` parameter.
    pub fn video_codecs(&self) -> String {
        self.codecs_where(true)
    }

    /// The codecs of its sound, as a MIME `codecs` parameter.
    pub fn audio_codecs(&self) -> String {
        self.codecs_where(false)
    }

    fn codecs_where(&self, video: bool) -> String {
        let chosen: Vec<&str> = self
            .codecs
            .split(',')
            .map(str::trim)
            .filter(|codec| !codec.is_empty() && is_video_codec(codec) == video)
            .collect();
        chosen.join(",")
    }
}

/// Whether `codec` (an RFC 6381 name) codes pictures.
fn is_video_codec(codec: &str) -> bool {
    [
        "avc1", "avc3", "hvc1", "hev1", "av01", "vp09", "vp8", "dvh1", "dvhe",
    ]
    .iter()
    .any(|prefix| codec.starts_with(prefix))
}

/// A master playlist: the renditions of one video, lowest bit rate first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Master {
    /// The renditions, by increasing bandwidth.
    pub variants: Vec<Variant>,
    /// The media playlist of the sound, when it comes apart from the pictures.
    pub audio: Option<String>,
}

/// One segment of a media playlist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    /// When it starts, in [`NANOS`].
    pub start: u64,
    /// How long it lasts, in [`NANOS`].
    pub duration: u64,
    /// Its bytes.
    pub resource: Resource,
}

impl Segment {
    /// When it ends, in [`NANOS`].
    pub fn end(&self) -> u64 {
        self.start.saturating_add(self.duration)
    }

    /// The middle of it, in [`NANOS`].
    pub fn middle(&self) -> u64 {
        self.start.saturating_add(self.duration / 2)
    }
}

/// A media playlist: the initialization section and the segments of one rendition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaPlaylist {
    /// The initialization section (`EXT-X-MAP`), which precedes the segments in a buffer.
    pub init: Option<Resource>,
    /// The segments, in order, each starting where the one before ends.
    pub segments: Vec<Segment>,
}

impl MediaPlaylist {
    /// How long the rendition lasts, in [`NANOS`].
    pub fn duration(&self) -> u64 {
        self.segments.last().map_or(0, Segment::end)
    }

    /// The segment that plays at `time` (in [`NANOS`]): the last whose start is not after it.
    pub fn segment_at(&self, time: u64) -> usize {
        self.segments
            .partition_point(|segment| segment.start <= time)
            .saturating_sub(1)
    }

    /// The first segment whose middle lies after `time`: what to fetch once everything up to
    /// `time` is buffered. Halfway is the tolerance, because a buffer ends a few frames before or
    /// after a segment's nominal end; the count of segments when nothing is left.
    pub fn first_after(&self, time: u64) -> usize {
        self.segments
            .partition_point(|segment| segment.middle() <= time)
    }
}

/// The lines of `text` that mean something, numbered from 1, after checking it is a playlist.
fn lines(text: &str) -> Result<impl Iterator<Item = (usize, &str)>, PlaylistError> {
    let mut all = text.lines().map(str::trim).enumerate();
    match all.next() {
        Some((_, "#EXTM3U")) => {}
        _ => return Err(PlaylistError::NotAPlaylist),
    }
    if text.lines().nth(MAX_LINES).is_some() {
        return Err(PlaylistError::TooLarge);
    }
    Ok(all
        .map(|(index, line)| (index.saturating_add(1), line))
        .filter(|(_, line)| !line.is_empty()))
}

/// Read the master playlist `text`, found at `base`.
pub fn parse_master(text: &str, base: &str) -> Result<Master, PlaylistError> {
    let mut variants = Vec::new();
    let mut audio = None;
    let mut pending: Option<Variant> = None;
    for (number, line) in lines(text)? {
        let bad = PlaylistError::Malformed { line: number };
        if let Some(list) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let attributes = attributes(list).ok_or(bad.clone())?;
            let bandwidth = attribute(&attributes, "BANDWIDTH")
                .and_then(|value| value.parse().ok())
                .ok_or(bad.clone())?;
            let (width, height) = match attribute(&attributes, "RESOLUTION") {
                Some(resolution) => {
                    let (w, h) = resolution.split_once('x').ok_or(bad.clone())?;
                    (
                        w.parse().map_err(|_| bad.clone())?,
                        h.parse().map_err(|_| bad.clone())?,
                    )
                }
                None => (0, 0),
            };
            let codecs = attribute(&attributes, "CODECS").unwrap_or_default();
            pending = Some(Variant {
                bandwidth,
                width,
                height,
                codecs: codecs.to_owned(),
                uri: String::new(),
            });
        } else if let Some(list) = line.strip_prefix("#EXT-X-MEDIA:") {
            let attributes = attributes(list).ok_or(bad)?;
            let default = attribute(&attributes, "DEFAULT") == Some("YES");
            if attribute(&attributes, "TYPE") == Some("AUDIO") && (default || audio.is_none()) {
                audio = attribute(&attributes, "URI").map(|uri| resolve(base, uri));
            }
        } else if !line.starts_with('#') {
            let Some(mut variant) = pending.take() else {
                return Err(bad);
            };
            if variants.len() >= MAX_VARIANTS {
                return Err(PlaylistError::TooLarge);
            }
            variant.uri = resolve(base, line);
            variants.push(variant);
        }
    }
    if variants.is_empty() {
        return Err(PlaylistError::Empty);
    }
    variants.sort_by_key(|variant| (variant.bandwidth, variant.height));
    Ok(Master { variants, audio })
}

/// Read the media playlist `text`, found at `base`.
pub fn parse_media(text: &str, base: &str) -> Result<MediaPlaylist, PlaylistError> {
    let mut init = None;
    let mut segments: Vec<Segment> = Vec::new();
    let mut duration: Option<u64> = None;
    let mut range: Option<(u64, Option<u64>)> = None;
    let mut start: u64 = 0;
    // Where the last byte range of each address ended, for a range that gives no offset.
    let mut ended: Option<(String, u64)> = None;
    for (number, line) in lines(text)? {
        let bad = PlaylistError::Malformed { line: number };
        if let Some(value) = line.strip_prefix("#EXTINF:") {
            let seconds = value.split(',').next().unwrap_or_default();
            duration = Some(nanos(seconds).ok_or(bad)?);
        } else if let Some(value) = line.strip_prefix("#EXT-X-BYTERANGE:") {
            range = Some(byte_range(value).ok_or(bad)?);
        } else if let Some(list) = line.strip_prefix("#EXT-X-MAP:") {
            let attributes = attributes(list).ok_or(bad.clone())?;
            let uri = attribute(&attributes, "URI").ok_or(bad.clone())?;
            let range = match attribute(&attributes, "BYTERANGE") {
                Some(value) => {
                    let (length, offset) = byte_range(value).ok_or(bad)?;
                    Some(ByteRange {
                        offset: offset.unwrap_or(0),
                        length,
                    })
                }
                None => None,
            };
            init = Some(Resource {
                uri: resolve(base, uri),
                range,
            });
        } else if !line.starts_with('#') {
            let length = duration.take().ok_or(bad.clone())?;
            let uri = resolve(base, line);
            let range = match range.take() {
                Some((length, offset)) => {
                    let offset = match offset {
                        Some(offset) => offset,
                        None => match &ended {
                            Some((last, end)) if *last == uri => *end,
                            _ => return Err(bad),
                        },
                    };
                    ended = Some((uri.clone(), offset.saturating_add(length)));
                    Some(ByteRange { offset, length })
                }
                None => None,
            };
            if segments.len() >= MAX_LINES {
                return Err(PlaylistError::TooLarge);
            }
            segments.push(Segment {
                start,
                duration: length,
                resource: Resource { uri, range },
            });
            start = start.checked_add(length).ok_or(PlaylistError::TooLarge)?;
        }
    }
    if segments.is_empty() {
        return Err(PlaylistError::Empty);
    }
    Ok(MediaPlaylist { init, segments })
}

/// `seconds`, a decimal such as `2.005333`, in nanoseconds; `None` if it is not a decimal of at
/// most nine places, or lasts beyond what 64 bits count.
pub fn nanos(seconds: &str) -> Option<u64> {
    let (whole, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
    let digits = |text: &str| text.bytes().all(|byte| byte.is_ascii_digit());
    if whole.is_empty() || fraction.len() > 9 || !digits(whole) || !digits(fraction) {
        return None;
    }
    let whole: u64 = whole.parse().ok()?;
    let padded = format!("{fraction:0<9}");
    let fraction: u64 = padded.parse().ok()?;
    whole.checked_mul(NANOS)?.checked_add(fraction)
}

/// `<length>[@<offset>]`.
fn byte_range(value: &str) -> Option<(u64, Option<u64>)> {
    let (length, offset) = match value.split_once('@') {
        Some((length, offset)) => (length, Some(offset.parse().ok()?)),
        None => (value, None),
    };
    let length: u64 = length.parse().ok()?;
    (length > 0).then_some((length, offset))
}

/// An attribute list, `NAME=value,NAME="quoted, value"`, as (name, value) pairs, quotes removed.
fn attributes(list: &str) -> Option<Vec<(&str, &str)>> {
    let mut pairs = Vec::new();
    let mut rest = list.trim();
    while !rest.is_empty() {
        let (name, after) = rest.split_once('=')?;
        let (value, after) = match after.strip_prefix('"') {
            Some(quoted) => {
                let (value, after) = quoted.split_once('"')?;
                (value, after)
            }
            None => after.split_once(',').map_or((after, ""), |(v, a)| (v, a)),
        };
        pairs.push((name.trim(), value));
        let after = after.trim_start();
        rest = after.strip_prefix(',').unwrap_or(after).trim_start();
        if pairs.len() > 64 {
            return None;
        }
    }
    Some(pairs)
}

fn attribute<'a>(pairs: &[(&str, &'a str)], name: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| *value)
}

/// `uri` as seen from the playlist at `base`: absolute addresses stay, relative ones replace
/// the last segment of `base`'s path.
pub fn resolve(base: &str, uri: &str) -> String {
    if uri.starts_with('/') || uri.contains("://") {
        return uri.to_owned();
    }
    let directory = base
        .rfind('/')
        .map_or("", |at| base.get(..=at).unwrap_or(""));
    format!("{directory}{uri}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const MASTER: &str = "#EXTM3U
#EXT-X-VERSION:7
#EXT-X-INDEPENDENT-SEGMENTS
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio\",NAME=\"Audio\",DEFAULT=YES,AUTOSELECT=YES,URI=\"audio.m3u8\"
#EXT-X-STREAM-INF:BANDWIDTH=2900000,AVERAGE-BANDWIDTH=1900000,RESOLUTION=1280x720,FRAME-RATE=29.970,CODECS=\"avc1.64001f,mp4a.40.2\",AUDIO=\"audio\"
720p.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=1500000,RESOLUTION=640x360,CODECS=\"avc1.64001e,mp4a.40.2\",AUDIO=\"audio\"
360p.m3u8
";

    const MEDIA: &str = "#EXTM3U
#EXT-X-VERSION:7
#EXT-X-TARGETDURATION:3
#EXT-X-PLAYLIST-TYPE:VOD
#EXT-X-MAP:URI=\"360p.mp4\",BYTERANGE=\"800@0\"
#EXTINF:2.002,
#EXT-X-BYTERANGE:1000@800
360p.mp4
#EXTINF:2.002000,
#EXT-X-BYTERANGE:1200
360p.mp4
#EXTINF:0.500500,
#EXT-X-BYTERANGE:300@3100
360p.mp4
#EXT-X-ENDLIST
";

    #[test]
    fn masters_list_renditions_lowest_first_with_their_sound() {
        let master = parse_master(MASTER, "/media/v/master.m3u8").unwrap();
        assert_eq!(master.audio.as_deref(), Some("/media/v/audio.m3u8"));
        let heights: Vec<u32> = master.variants.iter().map(|v| v.height).collect();
        assert_eq!(heights, [360, 720]);
        let low = &master.variants[0];
        assert_eq!(low.uri, "/media/v/360p.m3u8");
        assert_eq!(low.bandwidth, 1_500_000);
        assert_eq!(low.video_codecs(), "avc1.64001e");
        assert_eq!(low.audio_codecs(), "mp4a.40.2");
    }

    #[test]
    fn segments_start_at_the_exact_sum_of_the_durations_before() {
        let media = parse_media(MEDIA, "/media/v/360p.m3u8").unwrap();
        let init = media.init.clone().unwrap();
        assert_eq!(init.uri, "/media/v/360p.mp4");
        assert_eq!(
            init.range,
            Some(ByteRange {
                offset: 0,
                length: 800
            })
        );
        let starts: Vec<u64> = media.segments.iter().map(|s| s.start).collect();
        assert_eq!(starts, [0, 2_002_000_000, 4_004_000_000]);
        assert_eq!(media.duration(), 4_504_500_000);
        let second = media.segments[1].resource.range.unwrap();
        assert_eq!(
            second,
            ByteRange {
                offset: 1800,
                length: 1200
            }
        );
        assert_eq!(second.header(), "bytes=1800-2999");
    }

    #[test]
    fn adjacent_ranges_join_into_one_request() {
        let media = parse_media(MEDIA, "/v/360p.m3u8").unwrap();
        let init = media.init.clone().unwrap();
        let joined = init.joined(&media.segments[0].resource).unwrap();
        assert_eq!(
            joined.range,
            Some(ByteRange {
                offset: 0,
                length: 1800
            })
        );
        // The third segment does not follow the second's bytes.
        assert_eq!(
            media.segments[1]
                .resource
                .joined(&media.segments[2].resource),
            None
        );
        assert_eq!(init.joined(&media.segments[1].resource), None);
    }

    #[test]
    fn segments_are_found_by_time_and_by_what_is_buffered() {
        let media = parse_media(MEDIA, "/x/360p.m3u8").unwrap();
        assert_eq!(media.segment_at(0), 0);
        assert_eq!(media.segment_at(2_001_999_999), 0);
        assert_eq!(media.segment_at(2_002_000_000), 1);
        assert_eq!(media.segment_at(u64::MAX), 2);
        // A buffer ending a frame short of, or past, a segment's end asks for the next one.
        assert_eq!(media.first_after(2_002_000_000 - 33_000_000), 1);
        assert_eq!(media.first_after(2_002_000_000 + 21_000_000), 1);
        assert_eq!(media.first_after(4_504_500_000 - 10_000_000), 3);
    }

    #[test]
    fn decimals_read_exactly_or_not_at_all() {
        assert_eq!(nanos("2"), Some(2 * NANOS));
        assert_eq!(nanos("2.005333"), Some(2_005_333_000));
        assert_eq!(nanos("0.000000001"), Some(1));
        assert_eq!(nanos("0.0000000001"), None);
        assert_eq!(nanos("-1"), None);
        assert_eq!(nanos(".5"), None);
        assert_eq!(nanos("1e3"), None);
        assert_eq!(nanos("99999999999999999999"), None);
    }

    #[test]
    fn hostile_playlists_are_errors() {
        assert_eq!(parse_master("", "/"), Err(PlaylistError::NotAPlaylist));
        assert_eq!(parse_master("#EXTM3U\n", "/"), Err(PlaylistError::Empty));
        assert_eq!(
            parse_master("#EXTM3U\nlonely.m3u8\n", "/"),
            Err(PlaylistError::Malformed { line: 2 })
        );
        assert_eq!(
            parse_master("#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=x\na\n", "/"),
            Err(PlaylistError::Malformed { line: 2 })
        );
        assert_eq!(
            parse_media("#EXTM3U\n#EXT-X-BYTERANGE:10\n#EXTINF:1,\na.mp4\n", "/"),
            Err(PlaylistError::Malformed { line: 4 })
        );
        assert_eq!(
            parse_media("#EXTM3U\n#EXT-X-MAP:URI=\"unclosed\n", "/"),
            Err(PlaylistError::Malformed { line: 2 })
        );
        let huge = format!("#EXTM3U\n{}", "#\n".repeat(MAX_LINES + 1));
        assert_eq!(parse_media(&huge, "/"), Err(PlaylistError::TooLarge));
    }

    #[test]
    fn addresses_resolve_against_the_playlist() {
        assert_eq!(resolve("/media/v/master.m3u8", "a.m3u8"), "/media/v/a.m3u8");
        assert_eq!(resolve("/media/v/master.m3u8", "/b.mp4"), "/b.mp4");
        assert_eq!(resolve("master.m3u8", "a.m3u8"), "a.m3u8");
    }
}
