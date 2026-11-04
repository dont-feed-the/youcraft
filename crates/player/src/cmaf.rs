//! How long a video frame lasts, read exactly from a rendition's CMAF boxes: the timescale of
//! the picture track in its initialization section, and the duration of the first sample of its
//! first fragment, in ticks of that timescale.
//!
//! The bytes are the start of what the player fetches, so they are hostile and may end inside a
//! box: every size is checked, every walk is bounded by the bytes it walks, and anything that
//! does not read is `None`.

use std::num::NonZeroU32;

use youcraft_api::time::MediaDuration;

/// The boxes of `bytes`, in order: each one's type and body. A box claiming more bytes than
/// remain is the last, cut short.
fn boxes(bytes: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    let mut rest = bytes;
    std::iter::from_fn(move || {
        let size = u64::from(u32::from_be_bytes(rest.get(..4)?.try_into().ok()?));
        let kind: [u8; 4] = rest.get(4..8)?.try_into().ok()?;
        let (header, size) = match size {
            0 => (8, rest.len() as u64),
            1 => (16, u64::from_be_bytes(rest.get(8..16)?.try_into().ok()?)),
            size => (8, size),
        };
        if size < header {
            return None;
        }
        let end = usize::try_from(size).map_or(rest.len(), |size| size.min(rest.len()));
        let body = rest.get(usize::try_from(header).ok()?..end)?;
        rest = rest.get(end..).unwrap_or_default();
        Some((kind, body))
    })
}

/// The body of the first box of type `kind` in `bytes`.
fn child<'a>(bytes: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(bytes)
        .find(|(found, _)| found == kind)
        .map(|(_, body)| body)
}

/// The big-endian `u32` at `at` of `bytes`.
fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// A full box's version and flags.
fn version_and_flags(body: &[u8]) -> Option<(u8, u32)> {
    let word = u32_at(body, 0)?;
    Some(((word >> 24) as u8, word & 0x00ff_ffff))
}

/// The picture track of a `moov` body: its id and its timescale.
fn picture_track(moov: &[u8]) -> Option<(u32, u32)> {
    boxes(moov)
        .filter(|(kind, _)| kind == b"trak")
        .find_map(|(_, trak)| {
            let mdia = child(trak, b"mdia")?;
            // hdlr: version and flags, pre_defined, then the handler type.
            let handler = child(mdia, b"hdlr")?.get(8..12)?;
            if handler != b"vide" {
                return None;
            }
            let tkhd = child(trak, b"tkhd")?;
            let id = match version_and_flags(tkhd)?.0 {
                1 => u32_at(tkhd, 20)?,
                _ => u32_at(tkhd, 12)?,
            };
            let mdhd = child(mdia, b"mdhd")?;
            let timescale = match version_and_flags(mdhd)?.0 {
                1 => u32_at(mdhd, 20)?,
                _ => u32_at(mdhd, 12)?,
            };
            Some((id, timescale))
        })
}

/// The default sample duration a `moov` body's `trex` gives track `id`; 0 when none does.
fn trex_duration(moov: &[u8], id: u32) -> u32 {
    child(moov, b"mvex")
        .into_iter()
        .flat_map(boxes)
        .filter(|(kind, _)| kind == b"trex")
        .find(|(_, trex)| u32_at(trex, 4) == Some(id))
        .and_then(|(_, trex)| u32_at(trex, 12))
        .unwrap_or(0)
}

/// The duration of the first sample of track `id` in a `moof` body, from its `trun`, else its
/// `tfhd`, else `default`.
fn first_sample_duration(moof: &[u8], id: u32, default: u32) -> Option<u32> {
    let traf = boxes(moof)
        .filter(|(kind, _)| kind == b"traf")
        .map(|(_, traf)| traf)
        .find(|traf| child(traf, b"tfhd").and_then(|tfhd| u32_at(tfhd, 4)) == Some(id))?;
    let tfhd = child(traf, b"tfhd")?;
    let (_, tfhd_flags) = version_and_flags(tfhd)?;
    // tfhd: version and flags, track id, then the fields its flags say are present.
    let mut at = 8;
    if tfhd_flags & 0x01 != 0 {
        at += 8;
    }
    if tfhd_flags & 0x02 != 0 {
        at += 4;
    }
    let fragment_default = if tfhd_flags & 0x08 != 0 {
        u32_at(tfhd, at)?
    } else {
        default
    };
    let Some(trun) = child(traf, b"trun") else {
        return Some(fragment_default);
    };
    let (_, flags) = version_and_flags(trun)?;
    if u32_at(trun, 4)? == 0 || flags & 0x100 == 0 {
        return Some(fragment_default);
    }
    // trun: version and flags, sample count, then the optional data offset and first sample's
    // flags, then each sample's fields, the duration first.
    let mut at = 8;
    if flags & 0x01 != 0 {
        at += 4;
    }
    if flags & 0x04 != 0 {
        at += 4;
    }
    u32_at(trun, at)
}

/// How long a frame of the picture track lasts, read from `bytes`: a stream of boxes holding an
/// initialization section (`moov`) and then a fragment (`moof`); `None` unless both read.
pub fn frame_duration(bytes: &[u8]) -> Option<MediaDuration> {
    let moov = child(bytes, b"moov")?;
    let (id, timescale) = picture_track(moov)?;
    let moof = child(bytes, b"moof")?;
    let ticks = first_sample_duration(moof, id, trex_duration(moov, id))?;
    let timescale = NonZeroU32::new(timescale)?;
    (ticks > 0).then_some(MediaDuration {
        ticks: u64::from(ticks),
        timescale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed(kind: &[u8; 4], parts: &[&[u8]]) -> Vec<u8> {
        let body: Vec<u8> = parts.concat();
        let size = u32::try_from(body.len() + 8).unwrap();
        [&size.to_be_bytes()[..], kind, &body].concat()
    }

    fn word(value: u32) -> [u8; 4] {
        value.to_be_bytes()
    }

    /// `ticks` ticks of `timescale` a second.
    fn duration(ticks: u64, timescale: u32) -> MediaDuration {
        MediaDuration {
            ticks,
            timescale: NonZeroU32::new(timescale).unwrap(),
        }
    }

    /// A version-0 `moov` with one track of `handler`, id `id` and timescale `timescale`.
    fn moov(handler: &[u8; 4], id: u32, timescale: u32, trex: u32) -> Vec<u8> {
        let tkhd = boxed(
            b"tkhd",
            &[&word(3), &word(0), &word(0), &word(id), &[0; 64]],
        );
        let mdhd = boxed(
            b"mdhd",
            &[&word(0), &word(0), &word(0), &word(timescale), &[0; 8]],
        );
        let hdlr = boxed(b"hdlr", &[&word(0), &word(0), handler, &[0; 13]]);
        let mdia = boxed(b"mdia", &[&mdhd, &hdlr]);
        let trak = boxed(b"trak", &[&tkhd, &mdia]);
        let trex = boxed(
            b"trex",
            &[
                &word(0),
                &word(id),
                &word(1),
                &word(trex),
                &word(0),
                &word(0),
            ],
        );
        let mvex = boxed(b"mvex", &[&trex]);
        boxed(b"moov", &[&boxed(b"mvhd", &[&[0; 100]]), &trak, &mvex])
    }

    /// A `moof` for track `id` whose `trun` gives each of two samples `duration`, as the
    /// transcoder writes them (flags: data offset, duration, size, flags, composition offset).
    fn moof(id: u32, duration: u32) -> Vec<u8> {
        let tfhd = boxed(b"tfhd", &[&word(0x02_0000), &word(id)]);
        let tfdt = boxed(b"tfdt", &[&word(0x0100_0000), &[0; 8]]);
        let sample = [word(duration), word(1000), word(0x0200_0000), word(0)].concat();
        let trun = boxed(
            b"trun",
            &[&word(0x0f01), &word(2), &word(100), &sample, &sample],
        );
        let traf = boxed(b"traf", &[&tfhd, &tfdt, &trun]);
        boxed(b"moof", &[&boxed(b"mfhd", &[&word(0), &word(1)]), &traf])
    }

    #[test]
    fn the_frame_of_29_97_fps_is_3003_ticks_of_90_khz() {
        let mdat = boxed(b"mdat", &[&[0; 64]]);
        let stream = [
            boxed(b"ftyp", &[b"cmfc", &word(0)]),
            moov(b"vide", 1, 90_000, 0),
            moof(1, 3003),
            mdat,
        ]
        .concat();
        let frame = duration(3003, 90_000);
        assert_eq!(frame_duration(&stream), Some(frame));
        // A fetch's first bytes may end inside the media data: the boxes before it still read.
        assert_eq!(frame_duration(&stream[..stream.len() - 30]), Some(frame));
    }

    #[test]
    fn durations_come_from_the_fragment_or_the_track_defaults() {
        let tfhd = boxed(b"tfhd", &[&word(0x08), &word(1), &word(1001)]);
        let defaulted = boxed(b"moof", &[&boxed(b"traf", &[&tfhd])]);
        let stream = [moov(b"vide", 1, 24_000, 0), defaulted].concat();
        assert_eq!(frame_duration(&stream), Some(duration(1001, 24_000)));
        let bare = boxed(
            b"moof",
            &[&boxed(b"traf", &[&boxed(b"tfhd", &[&word(0), &word(1)])])],
        );
        let stream = [moov(b"vide", 1, 25, 1), bare].concat();
        assert_eq!(frame_duration(&stream), Some(duration(1, 25)));
    }

    #[test]
    fn sound_hostile_or_missing_boxes_give_nothing() {
        let sound = [moov(b"soun", 1, 48_000, 0), moof(1, 1024)].concat();
        assert_eq!(frame_duration(&sound), None);
        let other_track = [moov(b"vide", 1, 90_000, 0), moof(2, 3003)].concat();
        assert_eq!(frame_duration(&other_track), None);
        let zero = [moov(b"vide", 1, 90_000, 0), moof(1, 0)].concat();
        assert_eq!(frame_duration(&zero), None);
        assert_eq!(frame_duration(&moov(b"vide", 1, 90_000, 0)), None);
        assert_eq!(frame_duration(&[]), None);
        // Sizes smaller than a header, or larger than everything, end the walk.
        assert_eq!(frame_duration(&[0, 0, 0, 4, b'm', b'o', b'o', b'v']), None);
        assert_eq!(
            frame_duration(&[0, 0, 0, 1, b'm', b'o', b'o', b'v', 0xff, 0xff, 0xff, 0xff]),
            None
        );
        let mut huge = moov(b"vide", 1, 90_000, 0);
        huge.splice(0..4, u32::MAX.to_be_bytes());
        assert_eq!(frame_duration(&huge), None);
    }
}
