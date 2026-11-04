//! Which rendition to fetch next: the bandwidth the downloads show, and the choice it allows.
//!
//! Bandwidth is estimated as two exponentially weighted averages of the measured downloads, one
//! quick and one slow, and the lower of them is believed, so that quality falls as soon as the
//! network does and rises only once it has held. Each download weighs as long as it took, so many
//! small fetches do not outvote one large one. The choice starts at the lowest rendition, climbs
//! only with seconds of video already buffered, and never fetches pictures much larger than the
//! player shows them: a picture fits the player's box whole, so a wide one shown in a box of
//! another shape is shown smaller than the box.

use crate::playlist::Variant;
use crate::time::NANOS;

/// How long, in seconds, until a sample counts half as much in the quick average.
const QUICK_HALF_LIFE: f64 = 2.0;
/// The same for the slow average.
const SLOW_HALF_LIFE: f64 = 8.0;
/// Bytes to measure before the estimate is believed at all.
const MIN_BYTES: u64 = 128 * 1024;
/// Downloads smaller than this are mostly latency, and are not measured.
const MIN_SAMPLE_BYTES: u64 = 16 * 1024;
/// Nanoseconds of buffered video below which only a safe margin of the bandwidth is used.
const LOW_BUFFER: u64 = 10 * NANOS;
/// Nanoseconds of buffered video needed before choosing a higher rendition.
const CLIMB_BUFFER: u64 = 6 * NANOS;

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
    /// Nanoseconds of video buffered ahead of the playhead.
    pub buffered: u64,
    /// The player's box, width and height in device pixels; zero when unknown.
    pub shown: (u32, u32),
    /// The rendition being fetched now.
    pub current: usize,
}

/// The rendition to fetch next among `variants`, lowest bit rate first as
/// [`crate::playlist::Master`] orders them.
pub fn choose(variants: &[Variant], conditions: Conditions) -> usize {
    let Some(bandwidth) = conditions.bandwidth else {
        return 0;
    };
    let margin = if conditions.buffered < LOW_BUFFER {
        0.7
    } else {
        0.85
    };
    let ceiling = height_ceiling(variants, conditions.shown);
    let affordable = variants
        .iter()
        .enumerate()
        .filter(|(_, variant)| {
            variant.bandwidth as f64 <= bandwidth * margin
                && ceiling.is_none_or(|top| variant.height <= top)
        })
        .map(|(index, _)| index)
        .next_back()
        .unwrap_or(0);
    if affordable > conditions.current && conditions.buffered < CLIMB_BUFFER {
        return conditions.current.min(variants.len().saturating_sub(1));
    }
    affordable
}

/// How many device pixels tall a picture of `variant`'s shape shows in a box `shown`: the box's
/// height, or less when the picture is wider than the box and fits its width.
fn shown_height(variant: &Variant, (width, height): (u32, u32)) -> u32 {
    let fitted = (u64::from(width) * u64::from(variant.height))
        .checked_div(u64::from(variant.width))
        .map_or(height, |fitted| u32::try_from(fitted).unwrap_or(u32::MAX));
    height.min(fitted)
}

/// The tallest rendition worth fetching for a player box `shown`: the smallest that covers the
/// picture as it shows, so that nothing is fetched only to be scaled down; `None` lets any be.
fn height_ceiling(variants: &[Variant], shown: (u32, u32)) -> Option<u32> {
    if shown.1 == 0 {
        return None;
    }
    variants
        .iter()
        .filter(|variant| variant.height >= shown_height(variant, shown))
        .map(|variant| variant.height)
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renditions of `(bandwidth, width, height)`, lowest first.
    fn ladder(rungs: &[(u64, u32, u32)]) -> Vec<Variant> {
        rungs
            .iter()
            .map(|&(bandwidth, width, height)| Variant {
                bandwidth,
                width,
                height,
                frame: None,
                codecs: String::new(),
                uri: String::new(),
            })
            .collect()
    }

    fn wide() -> Vec<Variant> {
        ladder(&[
            (1_000_000, 640, 360),
            (2_500_000, 1280, 720),
            (5_000_000, 1920, 1080),
        ])
    }

    fn conditions(bandwidth: f64, buffered: u64, current: usize) -> Conditions {
        Conditions {
            bandwidth: Some(bandwidth),
            buffered: buffered * NANOS,
            shown: (4000, 2000),
            current,
        }
    }

    #[test]
    fn the_first_choice_is_the_lowest() {
        let estimator = Estimator::default();
        assert_eq!(estimator.estimate(), None);
        let first = Conditions {
            bandwidth: estimator.estimate(),
            buffered: 0,
            shown: (1920, 1080),
            current: 0,
        };
        assert_eq!(choose(&wide(), first), 0);
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
        let wide = wide();
        // Enough bandwidth for 1080p with a full buffer.
        assert_eq!(choose(&wide, conditions(8e6, 20, 0)), 2);
        // Too little buffer to climb: stay.
        assert_eq!(choose(&wide, conditions(8e6, 2, 0)), 0);
        // But falling never waits.
        assert_eq!(choose(&wide, conditions(1.5e6, 2, 2)), 0);
        // A low buffer keeps a wider margin: 2.5 Mbit/s needs 3.6 Mbit/s.
        assert_eq!(choose(&wide, conditions(3.4e6, 8, 1)), 0);
        assert_eq!(choose(&wide, conditions(3.4e6, 12, 1)), 1);
    }

    #[test]
    fn small_players_fetch_small_pictures() {
        let wide = wide();
        let small = Conditions {
            shown: (711, 400),
            ..conditions(1e9, 30, 0)
        };
        assert_eq!(choose(&wide, small), 1);
        let tiny = Conditions {
            shown: (356, 200),
            ..small
        };
        assert_eq!(choose(&wide, tiny), 0);
    }

    #[test]
    fn a_picture_wider_than_the_box_is_shown_shorter_than_it() {
        // 2.39:1 in a 16:9 box 1440 pixels tall shows 1071 pixels tall: 1080 lines suffice,
        // where the box alone would ask for 1440.
        let scope = ladder(&[
            (1_000_000, 860, 360),
            (2_500_000, 1720, 720),
            (5_000_000, 2580, 1080),
            (9_000_000, 3440, 1440),
        ]);
        let shown = Conditions {
            shown: (2560, 1440),
            ..conditions(1e9, 30, 0)
        };
        assert_eq!(choose(&scope, shown), 2);
        // In a box wider still, it shows 1190 pixels tall: more than 1080.
        let wider = Conditions {
            shown: (2844, 1600),
            ..shown
        };
        assert_eq!(choose(&scope, wider), 3);
        // An upright picture in a wide box is as tall as the box.
        let upright = ladder(&[(1_000_000, 360, 640), (2_500_000, 720, 1280)]);
        let landscape_box = Conditions {
            shown: (1920, 600),
            ..shown
        };
        assert_eq!(choose(&upright, landscape_box), 0);
    }
}
