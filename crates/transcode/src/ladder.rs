//! The rendition ladder: which sizes a video is made at, and at what bitrate.
//!
//! A rendition is named by its shorter side, as YouTube names a portrait 1080×1920 video "1080p".
//! A video gets every rung of [`RUNGS`] that its own shorter side reaches, never one above it, and
//! a video smaller than the lowest rung gets one rendition at its own size. Sizes keep the
//! picture's display aspect (pixel aspect applied), are even, as 4:2:0 needs, never exceed the
//! source, and no side exceeds [`MAX_SIDE`]: a picture so wide that a rung would is made smaller
//! and named by its own shorter side, and rungs that come out the same are made once.

/// The shorter sides renditions are made at, lowest first.
pub const RUNGS: [u32; 3] = [360, 720, 1080];

/// The longest side of a rendition: DCI 4K's width, within H.264 level 5.2, which every decoder
/// that plays 4K at 60 frames per second handles (and far below the encoder's 16 384).
pub const MAX_SIDE: u32 = 4096;

/// Bits per pixel and frame the encoder aims at; 1080p at 30 frames per second gets 5 Mbit/s.
const BITS_PER_PIXEL_PER_FRAME: u64 = 80;
const BITS_SCALE: u64 = 1000;

/// One size a video is made at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rung {
    /// Its name: the shorter side in pixels.
    pub label: u32,
    /// Its size in pixels, even.
    pub width: u32,
    /// Its size in pixels, even.
    pub height: u32,
    /// The average bitrate the encoder aims at, in kbit/s.
    pub target_kbps: u32,
    /// The bitrate it never exceeds over a second, in kbit/s.
    pub max_kbps: u32,
}

/// `side × num / den` rounded to the nearest even number, but never above `side` nor
/// [`MAX_SIDE`], and at least 2.
fn scaled(side: u32, num: u32, den: u32) -> u32 {
    let (side, num, den) = (u64::from(side), u64::from(num), u64::from(den.max(1)));
    // nearest even: twice the nearest whole number of pairs
    let even = 2 * ((side * num + den) / (2 * den));
    let limit = side.min(u64::from(MAX_SIDE)) & !1;
    u32::try_from(even.min(limit).max(2)).unwrap_or(2)
}

/// The rungs for a picture shown at `width`×`height` square pixels, played at `fps_num/fps_den`
/// frames per second.
pub fn ladder(width: u32, height: u32, fps_num: u32, fps_den: u32) -> Vec<Rung> {
    let (width, height) = (width.max(2), height.max(2));
    let (short, long) = (width.min(height), width.max(height));
    let mut labels: Vec<u32> = RUNGS
        .iter()
        .copied()
        .filter(|&rung| rung <= short)
        .collect();
    if labels.is_empty() {
        labels.push(short);
    }
    let mut rungs: Vec<Rung> = Vec::new();
    for label in labels {
        // the scale that gives the shorter side `label`, unless it takes the longer past MAX_SIDE
        let (num, den) =
            if u64::from(label) * u64::from(long) <= u64::from(MAX_SIDE) * u64::from(short) {
                (label, short)
            } else {
                (MAX_SIDE, long)
            };
        let (w, h) = (scaled(width, num, den), scaled(height, num, den));
        let label = w.min(h);
        if rungs.last().is_some_and(|lower| lower.label >= label) {
            continue;
        }
        let pixels = u64::from(w) * u64::from(h);
        let bits = pixels * BITS_PER_PIXEL_PER_FRAME * u64::from(fps_num)
            / u64::from(fps_den.max(1))
            / BITS_SCALE;
        let target_kbps = u32::try_from((bits / 1000).max(100)).unwrap_or(u32::MAX / 2);
        rungs.push(Rung {
            label,
            width: w,
            height: h,
            target_kbps,
            max_kbps: target_kbps.saturating_add(target_kbps / 2),
        });
    }
    rungs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sizes(rungs: &[Rung]) -> Vec<(u32, u32, u32)> {
        rungs.iter().map(|r| (r.label, r.width, r.height)).collect()
    }

    #[test]
    fn a_full_hd_video_gets_every_rung() {
        let rungs = ladder(1920, 1080, 30, 1);
        assert_eq!(
            sizes(&rungs),
            [(360, 640, 360), (720, 1280, 720), (1080, 1920, 1080)]
        );
        assert_eq!(rungs[2].target_kbps, 4976);
    }

    #[test]
    fn no_rung_is_above_the_source() {
        let labels: Vec<_> = ladder(1280, 720, 25, 1).iter().map(|r| r.label).collect();
        assert_eq!(labels, [360, 720]);
        assert_eq!(
            sizes(&ladder(1080, 1920, 30, 1)),
            [(360, 360, 640), (720, 720, 1280), (1080, 1080, 1920)]
        );
        // a 4K source stops at 1080p
        assert_eq!(
            sizes(&ladder(3840, 2160, 60, 1)).last(),
            Some(&(1080, 1920, 1080))
        );
    }

    #[test]
    fn a_tiny_or_odd_video_keeps_its_size_made_even_downwards() {
        let rungs = ladder(321, 241, 30_000, 1001);
        assert_eq!(sizes(&rungs), [(240, 320, 240)]);
        assert!(rungs[0].max_kbps > rungs[0].target_kbps);
        assert_eq!(sizes(&ladder(1, 1, 30, 1)), [(2, 2, 2)]);
        // an odd source keeps its aspect at every rung it reaches
        assert_eq!(
            sizes(&ladder(1919, 1079, 30, 1)),
            [(360, 640, 360), (720, 1280, 720)]
        );
    }

    #[test]
    fn a_very_wide_picture_is_bounded_by_the_longest_side() {
        // 32:9 fits within 4096 at 1080p
        assert_eq!(
            sizes(&ladder(3840, 1080, 30, 1)).last(),
            Some(&(1080, 3840, 1080))
        );
        // 6:1 does not: its 1080 rung becomes 4096×682, and the 720 rung stays 4320 → 4096 too
        let panorama = ladder(6480, 1080, 30, 1);
        assert_eq!(sizes(&panorama), [(360, 2160, 360), (682, 4096, 682)]);
        for rung in ladder(16 * 1080, 1080, 30, 1) {
            assert!(rung.width <= MAX_SIDE && rung.height <= MAX_SIDE);
        }
        // and so is a tall one
        assert_eq!(
            sizes(&ladder(1080, 6480, 30, 1)),
            [(360, 360, 2160), (682, 682, 4096)]
        );
    }
}
