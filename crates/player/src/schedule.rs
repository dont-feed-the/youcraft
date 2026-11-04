//! What to fetch next, read from what is buffered, never remembered: pure arithmetic on
//! nanoseconds, so that it is tested natively, and the engine only asks the browser what its
//! buffers hold.
//!
//! A segment counts as buffered when a buffered range holds its middle: half a segment absorbs a
//! buffer ending a few frames before or after a segment's nominal end. A track's buffer serves
//! the playhead when one range holds both the playhead, give or take [`SLACK`], and the middle of
//! the segment under it; the track then wants the first segment past that range, and otherwise
//! the segment under the playhead. Seeking, eviction, a gap before a buffered range and a change
//! of quality are thus all one case.

use crate::playlist::{MediaPlaylist, Segment};

/// Nanoseconds by which a buffered range may miss the playhead and still be under it: the
/// element stops at the end of what it holds, and a range may start a moment after the start of
/// its first segment.
pub const SLACK: u64 = 100_000_000;

/// A buffered range, from its start to its end, in nanoseconds.
pub type Range = (u64, u64);

/// The ranges of `ranges` under `time`, give or take [`SLACK`].
fn ranges_at(ranges: &[Range], time: u64) -> impl Iterator<Item = Range> + '_ {
    ranges.iter().copied().filter(move |&(start, end)| {
        start <= time.saturating_add(SLACK) && time <= end.saturating_add(SLACK)
    })
}

/// The buffered range under `time`, if any.
pub fn range_at(ranges: &[Range], time: u64) -> Option<Range> {
    ranges_at(ranges, time).next()
}

/// The buffered range that serves the playhead at `time` for `playlist`: under the playhead,
/// and holding the middle of the segment under it.
fn serving(playlist: &MediaPlaylist, ranges: &[Range], time: u64) -> Option<Range> {
    let middle = playlist.segments.get(playlist.segment_at(time))?.middle();
    ranges_at(ranges, time).find(|&(start, end)| start <= middle && middle <= end)
}

/// What one track's buffer holds for the playhead, and wants next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Want {
    /// Nanoseconds buffered from the playhead on, without a gap.
    pub ahead: u64,
    /// The segment to fetch next; `None` once none is left.
    pub next: Option<usize>,
    /// Whether the buffer holds everything from the playhead to the end.
    pub complete: bool,
}

/// What a track buffered over `ranges`, fed from `playlist`, wants for the playhead at `time`.
/// `last` is the segment appended last: a buffer ending short of its middle moves on rather than
/// fetch it again.
pub fn want(playlist: &MediaPlaylist, ranges: &[Range], time: u64, last: Option<usize>) -> Want {
    let run = serving(playlist, ranges, time);
    let mut next = match run {
        Some((_, end)) => playlist.first_after(end),
        None => playlist.segment_at(time),
    };
    if last == Some(next) {
        next = next.saturating_add(1);
    }
    let next = (next < playlist.segments.len()).then_some(next);
    Want {
        ahead: run.map_or(0, |(_, end)| end.saturating_sub(time)),
        next,
        complete: run.is_some() && next.is_none(),
    }
}

/// Which track to fetch for, and which segment: of those with something left, the one with the
/// least buffered ahead, if that is less than `target` nanoseconds.
pub fn neediest(wants: &[Want], target: u64) -> Option<(usize, usize)> {
    wants
        .iter()
        .enumerate()
        .filter_map(|(track, want)| Some((want.ahead, track, want.next?)))
        .filter(|(ahead, ..)| *ahead < target)
        .min()
        .map(|(_, track, segment)| (track, segment))
}

/// Where to cut a buffer so that a new rendition shows soon: the end of the segment under the
/// playhead at `time`.
pub fn flush_from(playlist: &MediaPlaylist, time: u64) -> u64 {
    playlist
        .segments
        .get(playlist.segment_at(time))
        .map_or(0, Segment::end)
}

/// Up to where to remove what lies behind the playhead at `time`: `keep` nanoseconds before it,
/// once the oldest moment buffered lies more than `after` (at least `keep`) behind it.
pub fn stale(ranges: &[Range], time: u64, keep: u64, after: u64) -> Option<u64> {
    let oldest = ranges.first()?.0;
    (oldest.saturating_add(after.max(keep)) < time).then(|| time.saturating_sub(keep))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playlist::Resource;
    use crate::time::NANOS;

    /// Ten segments of two seconds.
    fn playlist() -> MediaPlaylist {
        MediaPlaylist {
            init: None,
            segments: (0..10)
                .map(|k| Segment {
                    start: k * 2 * NANOS,
                    duration: 2 * NANOS,
                    resource: Resource {
                        uri: "360p.mp4".into(),
                        range: None,
                    },
                })
                .collect(),
        }
    }

    const MS: u64 = 1_000_000;

    #[test]
    fn the_next_segment_is_the_first_past_what_serves_the_playhead() {
        let list = playlist();
        let nothing = want(&list, &[], 0, None);
        assert_eq!(
            nothing,
            Want {
                ahead: 0,
                next: Some(0),
                complete: false
            }
        );
        let held = want(&list, &[(0, 6 * NANOS)], NANOS, Some(2));
        assert_eq!((held.ahead, held.next), (5 * NANOS, Some(3)));
        // A seek away from what is buffered fetches the segment under the new playhead.
        let away = want(&list, &[(0, 6 * NANOS)], 13 * NANOS, Some(2));
        assert_eq!((away.ahead, away.next), (0, Some(6)));
        // A range starting a moment late, as sound primed by an edit list can, still serves.
        let primed = want(&list, &[(21 * MS, 4 * NANOS + 10 * MS)], 0, Some(1));
        assert_eq!(primed.next, Some(2));
    }

    #[test]
    fn a_gap_before_a_buffered_range_is_fetched() {
        let list = playlist();
        // The playhead 50 ms before a range that starts with the next segment: the segment
        // under it is missing, though the range is within the slack.
        let gap = want(
            &list,
            &[(10 * NANOS, 12 * NANOS)],
            10 * NANOS - 50 * MS,
            None,
        );
        assert_eq!((gap.ahead, gap.next), (0, Some(4)));
        // A range trimmed in the middle of a segment, the playhead sought before its start.
        let trimmed = want(
            &list,
            &[(8 * NANOS + 500 * MS, 12 * NANOS)],
            8 * NANOS + 200 * MS,
            None,
        );
        assert_eq!(trimmed.next, Some(4));
        // The playhead stalled just past the end of what is buffered wants what follows.
        let stalled = want(&list, &[(0, 6 * NANOS)], 6 * NANOS + 50 * MS, Some(2));
        assert_eq!((stalled.ahead, stalled.next), (0, Some(3)));
    }

    #[test]
    fn a_segment_appended_short_is_not_fetched_again() {
        let list = playlist();
        // Segment 2 ([4 s, 6 s)) was appended last but its range ends before its middle.
        let short = want(&list, &[(0, 4 * NANOS + 900 * MS)], NANOS, Some(2));
        assert_eq!(short.next, Some(3));
        let again = want(&list, &[(0, 4 * NANOS + 900 * MS)], NANOS, Some(1));
        assert_eq!(again.next, Some(2));
    }

    #[test]
    fn a_buffer_holding_the_end_is_complete() {
        let list = playlist();
        let end = want(&list, &[(0, 20 * NANOS)], 15 * NANOS, Some(9));
        assert_eq!(
            end,
            Want {
                ahead: 5 * NANOS,
                next: None,
                complete: true
            }
        );
        // The last segment, appended but evicted under a playhead sought back to it, is not.
        let evicted = want(&list, &[(0, 4 * NANOS)], 19 * NANOS, Some(9));
        assert!(!evicted.complete);
    }

    #[test]
    fn the_track_with_least_ahead_is_fed_first() {
        let pictures = Want {
            ahead: 8 * NANOS,
            next: Some(5),
            complete: false,
        };
        let sound = Want {
            ahead: 6 * NANOS,
            next: Some(4),
            complete: false,
        };
        assert_eq!(neediest(&[pictures, sound], 30 * NANOS), Some((1, 4)));
        assert_eq!(neediest(&[pictures, sound], 5 * NANOS), None);
        let done = Want {
            next: None,
            ..sound
        };
        assert_eq!(neediest(&[pictures, done], 30 * NANOS), Some((0, 5)));
    }

    #[test]
    fn what_lies_far_behind_is_removed() {
        let list = playlist();
        assert_eq!(flush_from(&list, 5 * NANOS), 6 * NANOS);
        assert_eq!(flush_from(&list, 0), 2 * NANOS);
        let ranges = [(0, 100 * NANOS)];
        assert_eq!(stale(&ranges, 50 * NANOS, 30 * NANOS, 60 * NANOS), None);
        assert_eq!(
            stale(&ranges, 70 * NANOS, 30 * NANOS, 60 * NANOS),
            Some(40 * NANOS)
        );
        assert_eq!(stale(&ranges, 3 * NANOS, 2 * NANOS, 2 * NANOS), Some(NANOS));
        assert_eq!(stale(&ranges, NANOS, 2 * NANOS, 2 * NANOS), None);
        assert_eq!(stale(&[], 70 * NANOS, 30 * NANOS, 60 * NANOS), None);
        assert_eq!(
            range_at(&[(0, NANOS), (10 * NANOS, 12 * NANOS)], 11 * NANOS),
            Some((10 * NANOS, 12 * NANOS))
        );
    }
}
