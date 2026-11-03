//! Which rendition to fetch next: the bandwidth the downloads show, and the choice it allows.
//!
//! Bandwidth is estimated as two exponentially weighted averages of the measured downloads, one
//! quick and one slow, and the lower of them is believed, so that quality falls as soon as the
//! network does and rises only once it has held. Each download weighs as long as it took, so many
//! small fetches do not outvote one large one. The choice starts at the lowest rendition, climbs
//! only with seconds of video already buffered, and never fetches pictures much larger than the
//! player shows.

/// How long, in seconds, until a sample counts half as much in the quick average.
const QUICK_HALF_LIFE: f64 = 2.0;
/// The same for the slow average.
const SLOW_HALF_LIFE: f64 = 8.0;
/// Bytes to measure before the estimate is believed at all.
const MIN_BYTES: u64 = 128 * 1024;
/// Downloads smaller than this are mostly latency, and are not measured.
const MIN_SAMPLE_BYTES: u64 = 16 * 1024;
/// Seconds of buffered video below which only a safe margin of the bandwidth is used.
const LOW_BUFFER: f64 = 10.0;
/// Seconds of buffered video needed before choosing a higher rendition.
const CLIMB_BUFFER: f64 = 6.0;

/// An exponentially weighted average whose weights halve every `half_life` seconds of samples.
#[derive(Clone, Copy, Debug)]
struct Average {
    half_life: f64,
    value: f64,
    weight: f64,
}

impl Average {
    const fn new(half_life: f64) -> Average {
        Average {
            half_life,
            value: 0.0,
            weight: 0.0,
        }
    }

    fn add(&mut self, seconds: f64, value: f64) {
        let keep = 0.5_f64.powf(seconds / self.half_life);
        self.value = value * (1.0 - keep) + self.value * keep;
        self.weight = (1.0 - keep) + self.weight * keep;
    }

    /// The average, corrected for starting at zero.
    fn get(&self) -> Option<f64> {
        (self.weight > 0.0).then(|| self.value / self.weight)
    }
}

/// The bandwidth the downloads have shown.
#[derive(Clone, Copy, Debug)]
pub struct Estimator {
    quick: Average,
    slow: Average,
    measured: u64,
}

impl Default for Estimator {
    fn default() -> Estimator {
        Estimator {
            quick: Average::new(QUICK_HALF_LIFE),
            slow: Average::new(SLOW_HALF_LIFE),
            measured: 0,
        }
    }
}

impl Estimator {
    /// Count a download of `bytes` that took `millis` milliseconds.
    pub fn sample(&mut self, bytes: u64, millis: f64) {
        if bytes < MIN_SAMPLE_BYTES || !millis.is_finite() || millis <= 0.0 {
            return;
        }
        let seconds = millis / 1000.0;
        let bits_per_second = bytes as f64 * 8.0 / seconds;
        self.quick.add(seconds, bits_per_second);
        self.slow.add(seconds, bits_per_second);
        self.measured = self.measured.saturating_add(bytes);
    }

    /// The bandwidth to believe, in bits per second; `None` before enough was measured.
    pub fn estimate(&self) -> Option<f64> {
        if self.measured < MIN_BYTES {
            return None;
        }
        Some(self.quick.get()?.min(self.slow.get()?))
    }
}

/// What the choice depends on besides the renditions.
#[derive(Clone, Copy, Debug)]
pub struct Conditions {
    /// The bandwidth estimate, in bits per second.
    pub bandwidth: Option<f64>,
    /// Seconds of video buffered ahead of the playhead.
    pub buffered: f64,
    /// The height, in device pixels, the player shows pictures at.
    pub shown_height: u32,
    /// The rendition being fetched now.
    pub current: usize,
}

/// The rendition to fetch next, among renditions of `bandwidths` and `heights` (lowest bit rate
/// first, as [`crate::playlist::Master`] orders them).
pub fn choose(bandwidths: &[u64], heights: &[u32], conditions: Conditions) -> usize {
    let Some(bandwidth) = conditions.bandwidth else {
        return 0;
    };
    let margin = if conditions.buffered < LOW_BUFFER {
        0.7
    } else {
        0.85
    };
    let ceiling = height_ceiling(heights, conditions.shown_height);
    let affordable = bandwidths
        .iter()
        .zip(heights)
        .enumerate()
        .filter(|(_, (rate, height))| {
            **rate as f64 <= bandwidth * margin && ceiling.is_none_or(|top| **height <= top)
        })
        .map(|(index, _)| index)
        .next_back()
        .unwrap_or(0);
    if affordable > conditions.current && conditions.buffered < CLIMB_BUFFER {
        return conditions.current.min(bandwidths.len().saturating_sub(1));
    }
    affordable
}

/// The tallest rendition worth fetching for a player `shown` device pixels tall: the smallest
/// that covers it, so that nothing is fetched only to be scaled down; `None` lets any be.
fn height_ceiling(heights: &[u32], shown: u32) -> Option<u32> {
    if shown == 0 {
        return None;
    }
    heights.iter().copied().filter(|h| *h >= shown).min()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: [u64; 3] = [1_000_000, 2_500_000, 5_000_000];
    const HEIGHTS: [u32; 3] = [360, 720, 1080];

    fn conditions(bandwidth: f64, buffered: f64, current: usize) -> Conditions {
        Conditions {
            bandwidth: Some(bandwidth),
            buffered,
            shown_height: 2000,
            current,
        }
    }

    #[test]
    fn the_first_choice_is_the_lowest() {
        let estimator = Estimator::default();
        assert_eq!(estimator.estimate(), None);
        let first = Conditions {
            bandwidth: estimator.estimate(),
            buffered: 0.0,
            shown_height: 1080,
            current: 0,
        };
        assert_eq!(choose(&RATES, &HEIGHTS, first), 0);
    }

    #[test]
    fn steady_downloads_are_measured_as_they_are() {
        let mut estimator = Estimator::default();
        for _ in 0..20 {
            // 500 kB in half a second: 8 Mbit/s.
            estimator.sample(500_000, 500.0);
        }
        let estimate = estimator.estimate().unwrap();
        assert!((estimate - 8_000_000.0).abs() < 1.0, "{estimate}");
        // Latency-sized fetches change nothing.
        estimator.sample(1000, 1000.0);
        assert!((estimator.estimate().unwrap() - estimate).abs() < 1e-6);
    }

    #[test]
    fn a_slowdown_is_believed_at_once() {
        let mut estimator = Estimator::default();
        for _ in 0..20 {
            estimator.sample(500_000, 500.0);
        }
        estimator.sample(500_000, 4000.0);
        assert!(estimator.estimate().unwrap() < 5_000_000.0);
    }

    #[test]
    fn quality_climbs_with_bandwidth_and_buffer_only() {
        // Enough bandwidth for 1080p with a full buffer.
        assert_eq!(choose(&RATES, &HEIGHTS, conditions(8e6, 20.0, 0)), 2);
        // Too little buffer to climb: stay.
        assert_eq!(choose(&RATES, &HEIGHTS, conditions(8e6, 2.0, 0)), 0);
        // But falling never waits.
        assert_eq!(choose(&RATES, &HEIGHTS, conditions(1.5e6, 2.0, 2)), 0);
        // A low buffer keeps a wider margin: 2.5 Mbit/s needs 3.6 Mbit/s.
        assert_eq!(choose(&RATES, &HEIGHTS, conditions(3.4e6, 8.0, 1)), 0);
        assert_eq!(choose(&RATES, &HEIGHTS, conditions(3.4e6, 12.0, 1)), 1);
    }

    #[test]
    fn small_players_fetch_small_pictures() {
        let small = Conditions {
            shown_height: 400,
            ..conditions(1e9, 30.0, 0)
        };
        assert_eq!(choose(&RATES, &HEIGHTS, small), 1);
        let tiny = Conditions {
            shown_height: 200,
            ..small
        };
        assert_eq!(choose(&RATES, &HEIGHTS, tiny), 0);
    }
}
