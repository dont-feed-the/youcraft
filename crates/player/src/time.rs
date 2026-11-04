//! Media time in the player: whole nanoseconds, as the playlists count it. The element counts
//! floating-point seconds; they become nanoseconds here, at its boundary, and nowhere else. A
//! frame lasts an exact rational number of seconds, so stepping frames is done in 128-bit
//! integers of its own timescale, never by adding floating-point seconds.

use std::num::NonZeroU32;

use youcraft_api::time::MediaDuration;

/// The timescale of every time in the player: nanoseconds, exact for any decimal of nine places,
/// as a playlist writes them.
const NANOSECONDS: NonZeroU32 = match NonZeroU32::new(1_000_000_000) {
    Some(timescale) => timescale,
    None => NonZeroU32::MIN,
};

/// Ticks per second of every time in the player.
pub const NANOS: u64 = NANOSECONDS.get() as u64;

/// The element's `seconds` as nanoseconds, to the nearest; negative and NaN as 0, beyond what 64
/// bits count as the largest they do.
pub fn from_seconds(seconds: f64) -> u64 {
    // A float-to-integer `as` saturates, and takes NaN to 0.
    (seconds * NANOS as f64).round() as u64
}

/// `nanos` as the floating-point seconds the element counts.
pub fn seconds(nanos: u64) -> f64 {
    nanos as f64 / NANOS as f64
}

/// The whole seconds of `nanos`, as a clock shows them.
pub fn whole_seconds(nanos: u64) -> u64 {
    nanos / NANOS
}

/// `duration` in nanoseconds, rounded down; beyond what 64 bits count, the largest they do.
pub fn of(duration: MediaDuration) -> u64 {
    duration
        .in_timescale(NANOSECONDS)
        .map_or(u64::MAX, |duration| duration.ticks)
}

/// `numerator / denominator` of 128 bits, when it is defined and fits 64.
fn quotient(numerator: Option<u128>, denominator: Option<u128>) -> Option<u64> {
    u64::try_from(numerator?.checked_div(denominator?)?).ok()
}

/// How many whole `period`s lie in `time`: ⌊time / period⌋, the index of the frame, or of the
/// storyboard tile, showing at `time`; `None` when `period` is not a duration.
pub fn periods(time: u64, period: MediaDuration) -> Option<u64> {
    quotient(
        u128::from(time).checked_mul(u128::from(period.timescale.get())),
        u128::from(NANOS).checked_mul(u128::from(period.ticks)),
    )
}

/// How many `period`s it takes to cover `time`: ⌈time / period⌉.
pub fn periods_covering(time: u64, period: MediaDuration) -> Option<u64> {
    let denominator = u128::from(NANOS).checked_mul(u128::from(period.ticks))?;
    quotient(
        u128::from(time)
            .checked_mul(u128::from(period.timescale.get()))
            .and_then(|numerator| numerator.checked_add(denominator.checked_sub(1)?)),
        Some(denominator),
    )
}

/// The middle of frame `index`, in nanoseconds, rounded down: `(index + ½) × frame`. Seeking
/// there shows that frame whatever the element rounds.
pub fn frame_middle(index: u64, frame: MediaDuration) -> Option<u64> {
    quotient(
        u128::from(index)
            .checked_mul(2)
            .and_then(|twice| twice.checked_add(1))
            .and_then(|halves| halves.checked_mul(u128::from(frame.ticks)))
            .and_then(|halves| halves.checked_mul(u128::from(NANOS))),
        u128::from(frame.timescale.get()).checked_mul(2),
    )
}

/// Where stepping `frames` frames from `time` lands, for frames lasting `frame` each: the middle
/// of the frame that many after (or, when negative, before) the one showing, never before the
/// first; `None` when `frame` is not a duration.
pub fn step_frames(time: u64, frames: i32, frame: MediaDuration) -> Option<u64> {
    let at = periods(time, frame)?;
    let to = if frames < 0 {
        at.saturating_sub(u64::from(frames.unsigned_abs()))
    } else {
        at.saturating_add(u64::from(frames.unsigned_abs()))
    };
    frame_middle(to, frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ticks` ticks of `timescale` a second.
    const fn duration(ticks: u64, timescale: u32) -> MediaDuration {
        MediaDuration {
            ticks,
            timescale: NonZeroU32::new(timescale).unwrap(),
        }
    }

    /// A frame of 29.97 frames a second, in the 90 kHz timescale the transcoder writes.
    const NTSC: MediaDuration = duration(3003, 90_000);

    #[test]
    fn the_elements_seconds_round_to_the_nearest_nanosecond() {
        assert_eq!(from_seconds(2.002), 2_002_000_000);
        assert_eq!(from_seconds(-1.0), 0);
        assert_eq!(from_seconds(f64::NAN), 0);
        assert_eq!(from_seconds(f64::INFINITY), u64::MAX);
        assert_eq!(seconds(1_500_000_000), 1.5);
        assert_eq!(whole_seconds(2_999_999_999), 2);
        assert_eq!(of(duration(1001, 30_000)), 33_366_666);
        assert_eq!(of(duration(u64::MAX, 1)), u64::MAX);
    }

    #[test]
    fn frames_step_from_middle_to_middle_exactly() {
        // Frame k of 29.97 fps starts at k × 1001/30000 s.
        assert_eq!(periods(0, NTSC), Some(0));
        assert_eq!(periods(33_366_666, NTSC), Some(0));
        assert_eq!(periods(33_366_667, NTSC), Some(1));
        assert_eq!(periods_covering(33_366_666, NTSC), Some(1));
        assert_eq!(periods_covering(0, NTSC), Some(0));
        assert_eq!(periods_covering(100_100_000, NTSC), Some(3));
        assert_eq!(periods_covering(100_100_001, NTSC), Some(4));
        assert_eq!(frame_middle(0, NTSC), Some(16_683_333));
        assert_eq!(step_frames(0, 1, NTSC), Some(50_050_000));
        assert_eq!(step_frames(50_050_000, -1, NTSC), Some(16_683_333));
        assert_eq!(step_frames(16_683_333, -1, NTSC), Some(16_683_333));
        // After an hour, the step still lands in the middle of the next frame: no drift.
        let hour = 3600 * NANOS;
        let at = periods(hour, NTSC).unwrap();
        assert_eq!(at, 107_892);
        let next = step_frames(hour, 1, NTSC).unwrap();
        assert_eq!(periods(next, NTSC), Some(at + 1));
        assert!(next > hour && next - hour < 2 * 33_366_667);
        // 24, 25 and 60 frames a second step by their own frames, not by thirtieths.
        for fps in [24, 25, 60] {
            let frame = duration(1, fps);
            let first = step_frames(0, 1, frame).unwrap();
            assert_eq!(first, NANOS * 3 / (2 * u64::from(fps)));
        }
        let none = duration(0, 90_000);
        assert_eq!(step_frames(NANOS, 1, none), None);
        assert_eq!(step_frames(u64::MAX, i32::MAX, NTSC), None);
    }
}
