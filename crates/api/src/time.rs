//! Exact media time: whole ticks of a timescale, never floating-point seconds.

use std::num::NonZeroU32;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The timescale of milliseconds.
pub const MILLISECONDS: NonZeroU32 = match NonZeroU32::new(1000) {
    Some(timescale) => timescale,
    None => NonZeroU32::MIN,
};

/// A length of media time: `ticks` ticks of `1 / timescale` seconds each.
///
/// Two durations are equal when both their ticks and their timescales are: `1000/1000` and
/// `1/1` last as long, but are different records. Compare lengths in one timescale, through
/// [`MediaDuration::in_timescale`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct MediaDuration {
    /// How many ticks.
    pub ticks: u64,
    /// Ticks per second.
    pub timescale: NonZeroU32,
}

impl MediaDuration {
    /// A duration of whole milliseconds.
    pub const fn from_millis(millis: u64) -> MediaDuration {
        MediaDuration {
            ticks: millis,
            timescale: MILLISECONDS,
        }
    }

    /// The whole seconds it lasts, rounded down, as a page shows it (`12:34`).
    pub fn whole_seconds(self) -> u64 {
        self.ticks / u64::from(self.timescale.get())
    }

    /// The same duration in another timescale, rounded down to whole ticks; computed in 128 bits,
    /// so it overflows only when the result does not fit.
    pub fn in_timescale(self, timescale: NonZeroU32) -> Option<MediaDuration> {
        let ticks =
            u128::from(self.ticks) * u128::from(timescale.get()) / u128::from(self.timescale.get());
        Some(MediaDuration {
            ticks: u64::try_from(ticks).ok()?,
            timescale,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scale(timescale: u32) -> NonZeroU32 {
        NonZeroU32::new(timescale).unwrap()
    }

    #[test]
    fn seconds_round_down() {
        assert_eq!(
            MediaDuration {
                ticks: 90_000 * 754 + 89_999,
                timescale: scale(90_000)
            }
            .whole_seconds(),
            754
        );
        assert_eq!(MediaDuration::from_millis(1999).whole_seconds(), 1);
    }

    #[test]
    fn timescales_convert_exactly_when_they_divide() {
        let video = MediaDuration {
            ticks: 1001 * 300,
            timescale: scale(30_000),
        };
        assert_eq!(
            video.in_timescale(scale(1000)),
            Some(MediaDuration {
                ticks: 10_010,
                timescale: scale(1000)
            })
        );
        assert_eq!(
            MediaDuration {
                ticks: u64::MAX,
                timescale: scale(1)
            }
            .in_timescale(scale(2)),
            None
        );
    }

    #[test]
    fn a_zero_timescale_never_parses() {
        let parsed: Result<MediaDuration, _> = serde_json::from_str(r#"{"ticks":5,"timescale":0}"#);
        assert!(parsed.is_err());
        let parsed: MediaDuration =
            serde_json::from_str(r#"{"ticks":5,"timescale":48000}"#).unwrap();
        assert_eq!(parsed.timescale.get(), 48_000);
    }
}
