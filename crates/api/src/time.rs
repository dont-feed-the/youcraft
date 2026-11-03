//! Exact media time: whole ticks of a timescale, never floating-point seconds.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A length of media time: `ticks` ticks of `1 / timescale` seconds each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct MediaDuration {
    /// How many ticks.
    pub ticks: u64,
    /// Ticks per second; never zero.
    pub timescale: u32,
}

impl MediaDuration {
    /// A duration of whole milliseconds.
    pub const fn from_millis(millis: u64) -> MediaDuration {
        MediaDuration {
            ticks: millis,
            timescale: 1000,
        }
    }

    /// The whole seconds it lasts, rounded down, as a page shows it (`12:34`); zero for a zero
    /// timescale.
    pub fn whole_seconds(self) -> u64 {
        self.ticks
            .checked_div(u64::from(self.timescale))
            .unwrap_or(0)
    }

    /// The same duration in another timescale, rounded down to whole ticks; computed in 128 bits,
    /// so it never overflows on the way.
    pub fn in_timescale(self, timescale: u32) -> Option<MediaDuration> {
        let ticks = u128::from(self.ticks) * u128::from(timescale);
        let ticks = ticks.checked_div(u128::from(self.timescale))?;
        Some(MediaDuration {
            ticks: u64::try_from(ticks).ok()?,
            timescale,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_round_down() {
        assert_eq!(
            MediaDuration {
                ticks: 90_000 * 754 + 89_999,
                timescale: 90_000
            }
            .whole_seconds(),
            754
        );
        assert_eq!(
            MediaDuration {
                ticks: 5,
                timescale: 0
            }
            .whole_seconds(),
            0
        );
    }

    #[test]
    fn timescales_convert_exactly_when_they_divide() {
        let video = MediaDuration {
            ticks: 1001 * 300,
            timescale: 30_000,
        };
        assert_eq!(
            video.in_timescale(1000),
            Some(MediaDuration {
                ticks: 10_010,
                timescale: 1000
            })
        );
        assert_eq!(
            MediaDuration {
                ticks: u64::MAX,
                timescale: 1
            }
            .in_timescale(2),
            None
        );
    }
}
