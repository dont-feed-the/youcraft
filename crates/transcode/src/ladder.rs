//! The rendition ladder: which sizes a video is made at, and at what bitrate.
//!
//! A rendition is named by its shorter side, as YouTube names a portrait 1080×1920 video "1080p".
//! A video gets every rung of [`RUNGS`] that its own shorter side reaches, never one above it, and
//! a video smaller than the lowest rung gets one rendition at its own size. Sizes keep the
//! picture's display aspect (pixel aspect applied) and are even, as 4:2:0 needs.

/// The shorter sides renditions are made at, lowest first.
pub const RUNGS: [u32; 3] = [360, 720, 1080];

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

/// `value` rounded to the nearest even number, at least 2.
fn even(value: u64) -> u32 {
    let rounded = value.saturating_add(1) / 2 * 2;
    u32::try_from(rounded.max(2)).unwrap_or(u32::MAX - 1)
}

/// The rungs for a picture shown at `width`×`height` square pixels, played at `fps_num/fps_den`
/// frames per second.
pub fn ladder(width: u32, height: u32, fps_num: u32, fps_den: u32) -> Vec<Rung> {
    let (width, height) = (width.max(2), height.max(2));
    let short = width.min(height);
    let mut labels: Vec<u32> = RUNGS
        .iter()
        .copied()
        .filter(|&rung| rung <= short)
        .collect();
    if labels.is_empty() {
        labels.push(short);
    }
    labels
        .into_iter()
        .map(|label| {
            let scale = |side: u32| even(u64::from(side) * u64::from(label) / u64::from(short));
            let (w, h) = (scale(width), scale(height));
            let pixels = u64::from(w) * u64::from(h);
            let bits = pixels * BITS_PER_PIXEL_PER_FRAME * u64::from(fps_num)
                / u64::from(fps_den.max(1))
                / BITS_SCALE;
            let target_kbps = u32::try_from((bits / 1000).max(100)).unwrap_or(u32::MAX / 2);
            Rung {
                label,
                width: w,
                height: h,
                target_kbps,
                max_kbps: target_kbps.saturating_add(target_kbps / 2),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_hd_video_gets_every_rung() {
        let rungs = ladder(1920, 1080, 30, 1);
        let sizes: Vec<_> = rungs.iter().map(|r| (r.label, r.width, r.height)).collect();
        assert_eq!(
            sizes,
            [(360, 640, 360), (720, 1280, 720), (1080, 1920, 1080)]
        );
        assert_eq!(rungs[2].target_kbps, 4976);
    }

    #[test]
    fn no_rung_is_above_the_source() {
        let sizes: Vec<_> = ladder(1280, 720, 25, 1).iter().map(|r| r.label).collect();
        assert_eq!(sizes, [360, 720]);
        let portrait: Vec<_> = ladder(1080, 1920, 30, 1)
            .iter()
            .map(|r| (r.width, r.height))
            .collect();
        assert_eq!(portrait, [(360, 640), (720, 1280), (1080, 1920)]);
    }

    #[test]
    fn a_tiny_video_keeps_its_size_made_even() {
        let rungs = ladder(321, 241, 30_000, 1001);
        assert_eq!(rungs.len(), 1);
        assert_eq!(
            (rungs[0].label, rungs[0].width, rungs[0].height),
            (241, 322, 242)
        );
        assert!(rungs[0].max_kbps > rungs[0].target_kbps);
    }
}
