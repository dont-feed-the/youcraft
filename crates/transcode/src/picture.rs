//! Pictures between the decoder and the encoders: resizing, letterboxing and BT.709 4:2:0.
//!
//! [`Rgba`] holds straight sRGB-encoded RGBA8, as FilmCraft's frames convert to. Resizing is a
//! separable triangle filter as wide as the scale factor (so a downscale averages every source
//! pixel it covers) in integer arithmetic, so a size change is exact and repeatable: weights of
//! each output pixel sum to exactly one. [`Yuv420`] is what the H.264 encoder takes: limited-range
//! BT.709 Y'CbCr from the same R'G'B', chroma the mean of each 2×2 block.

/// Weights are fixed point with this many fractional bits.
const WEIGHT_BITS: u32 = 14;
const ONE: u32 = 1 << WEIGHT_BITS;
/// The horizontal pass keeps this many fractional bits for the vertical one.
const KEEP_BITS: u32 = 8;

/// A straight sRGB RGBA8 picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    /// Its size in pixels.
    pub width: u32,
    /// Its size in pixels.
    pub height: u32,
    /// Four bytes per pixel, rows top to bottom.
    pub pixels: Vec<u8>,
}

/// A limited-range BT.709 4:2:0 picture of even size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Yuv420 {
    /// Its size in pixels, even.
    pub width: u32,
    /// Its size in pixels, even.
    pub height: u32,
    /// Luma, `width × height`.
    pub y: Vec<u8>,
    /// Blue difference, `width/2 × height/2`.
    pub u: Vec<u8>,
    /// Red difference, `width/2 × height/2`.
    pub v: Vec<u8>,
}

/// The input pixels one output pixel reads: from `first`, one weight each.
struct Taps {
    first: usize,
    weights: Vec<u32>,
}

/// The taps of every output position when `from` samples become `to`.
fn taps(from: usize, to: usize) -> Vec<Taps> {
    let (from_f, to_f) = (from.max(1) as f64, to.max(1) as f64);
    let scale = from_f / to_f;
    let radius = scale.max(1.0);
    let last = from.saturating_sub(1) as i64;
    (0..to)
        .map(|x| {
            let center = (x as f64 + 0.5) * scale - 0.5;
            let low = ((center - radius).floor() as i64 + 1).clamp(0, last);
            let high = ((center + radius).ceil() as i64 - 1).clamp(low, last);
            let raw: Vec<f64> = (low..=high)
                .map(|i| (1.0 - (i as f64 - center).abs() / radius).max(0.0))
                .collect();
            let total: f64 = raw.iter().sum();
            let mut weights: Vec<u32> = raw
                .iter()
                .map(|w| {
                    if total > 0.0 {
                        (w / total * f64::from(ONE)).round() as u32
                    } else {
                        0
                    }
                })
                .collect();
            // what rounding lost or added goes to the heaviest tap, so the sum is exactly one
            let sum: u32 = weights.iter().sum();
            if let Some(heaviest) = weights.iter_mut().max() {
                *heaviest = (*heaviest + ONE).saturating_sub(sum);
            } else {
                weights.push(ONE);
            }
            Taps {
                first: usize::try_from(low).unwrap_or(0),
                weights,
            }
        })
        .collect()
}

impl Rgba {
    /// A picture of one colour.
    pub fn filled(width: u32, height: u32, rgba: [u8; 4]) -> Rgba {
        let count = width as usize * height as usize;
        Rgba {
            width,
            height,
            pixels: rgba.repeat(count),
        }
    }

    /// The same picture at `width`×`height`.
    pub fn resized(&self, width: u32, height: u32) -> Rgba {
        if (width, height) == (self.width, self.height) {
            return self.clone();
        }
        let (in_w, in_h) = (self.width as usize, self.height as usize);
        let (out_w, out_h) = (width as usize, height as usize);
        let across = taps(in_w, out_w);
        let mut rows: Vec<Vec<u32>> = Vec::with_capacity(in_h);
        for row in self.pixels.chunks_exact(in_w.max(1) * 4).take(in_h) {
            let mut out = Vec::with_capacity(out_w * 4);
            for tap in &across {
                let mut sum = [0u32; 4];
                let pixels = row
                    .get(tap.first * 4..)
                    .unwrap_or_default()
                    .as_chunks::<4>()
                    .0;
                for (pixel, weight) in pixels.iter().zip(&tap.weights) {
                    for (s, &p) in sum.iter_mut().zip(pixel) {
                        *s += u32::from(p) * weight;
                    }
                }
                let shift = WEIGHT_BITS - KEEP_BITS;
                out.extend(sum.map(|s| (s + (1 << (shift - 1))) >> shift));
            }
            rows.push(out);
        }
        let mut pixels = Vec::with_capacity(out_w * out_h * 4);
        let mut sum = vec![0u64; out_w * 4];
        for tap in taps(in_h, out_h) {
            sum.fill(0);
            for (row, &weight) in rows.iter().skip(tap.first).zip(&tap.weights) {
                for (s, &value) in sum.iter_mut().zip(row) {
                    *s += u64::from(value) * u64::from(weight);
                }
            }
            let shift = WEIGHT_BITS + KEEP_BITS;
            pixels.extend(
                sum.iter()
                    .map(|&s| ((s + (1 << (shift - 1))) >> shift).min(255) as u8),
            );
        }
        Rgba {
            width,
            height,
            pixels,
        }
    }

    /// The picture scaled to fit inside `width`×`height`, centred on black, as a thumbnail shows
    /// a picture of another shape.
    pub fn fitted(&self, width: u32, height: u32) -> Rgba {
        let (w, h) = (u64::from(self.width.max(1)), u64::from(self.height.max(1)));
        let (bw, bh) = (u64::from(width), u64::from(height));
        let (fit_w, fit_h) = if w * bh >= h * bw {
            (bw, (h * bw + w / 2) / w)
        } else {
            ((w * bh + h / 2) / h, bh)
        };
        let (fit_w, fit_h) = (fit_w.clamp(1, bw) as u32, fit_h.clamp(1, bh) as u32);
        let inner = self.resized(fit_w, fit_h);
        if (fit_w, fit_h) == (width, height) {
            return inner;
        }
        let mut canvas = Rgba::filled(width, height, [0, 0, 0, 255]);
        canvas.paste(&inner, (width - fit_w) / 2, (height - fit_h) / 2);
        canvas
    }

    /// Copy `other` onto this picture with its top-left corner at `x`, `y`, clipped to the edges.
    pub fn paste(&mut self, other: &Rgba, x: u32, y: u32) {
        let (stride, other_stride) = (self.width as usize * 4, other.width as usize * 4);
        let start = x as usize * 4;
        let rows = self.pixels.chunks_exact_mut(stride.max(1)).skip(y as usize);
        for (row, source) in rows.zip(other.pixels.chunks_exact(other_stride.max(1))) {
            let room = stride.saturating_sub(start).min(source.len());
            if let (Some(target), Some(source)) =
                (row.get_mut(start..start + room), source.get(..room))
            {
                target.copy_from_slice(source);
            }
        }
    }

    /// The picture as limited-range BT.709 4:2:0; its size must be even.
    pub fn to_yuv420(&self) -> Yuv420 {
        let (w, h) = (self.width as usize, self.height as usize);
        let mut y = Vec::with_capacity(w * h);
        for &[r, g, b, _] in self.pixels.as_chunks::<4>().0 {
            y.push(luma(i64::from(r), i64::from(g), i64::from(b)));
        }
        let (cw, ch) = (w / 2, h / 2);
        let (mut u, mut v) = (Vec::with_capacity(cw * ch), Vec::with_capacity(cw * ch));
        let mut rows = self.pixels.chunks_exact((w * 4).max(1));
        for _ in 0..ch {
            let (Some(top), Some(bottom)) = (rows.next(), rows.next()) else {
                break;
            };
            let pairs = top.as_chunks::<8>().0.iter().zip(bottom.as_chunks::<8>().0);
            for (&[r0, g0, b0, _, r1, g1, b1, _], &[r2, g2, b2, _, r3, g3, b3, _]) in pairs.take(cw)
            {
                let sum = |a: u8, b: u8, c: u8, d: u8| {
                    i64::from(a) + i64::from(b) + i64::from(c) + i64::from(d)
                };
                let (r, g, b) = (
                    sum(r0, r1, r2, r3),
                    sum(g0, g1, g2, g3),
                    sum(b0, b1, b2, b3),
                );
                u.push(chroma(CB, r, g, b));
                v.push(chroma(CR, r, g, b));
            }
        }
        Yuv420 {
            width: self.width,
            height: self.height,
            y,
            u,
            v,
        }
    }
}

/// BT.709's luma weights of R' and B'; G' takes the rest.
const KR: f64 = 0.2126;
const KB: f64 = 0.0722;
const FIXED: f64 = 65536.0;
/// Limited range: luma spans 219 codes from 16, chroma 224 around 128.
const Y_RGB: [i64; 3] = [
    (KR * 219.0 / 255.0 * FIXED + 0.5) as i64,
    ((1.0 - KR - KB) * 219.0 / 255.0 * FIXED + 0.5) as i64,
    (KB * 219.0 / 255.0 * FIXED + 0.5) as i64,
];
const CB_SCALE: f64 = 224.0 / 255.0 / (2.0 * (1.0 - KB)) * FIXED;
const CR_SCALE: f64 = 224.0 / 255.0 / (2.0 * (1.0 - KR)) * FIXED;
const CB: [i64; 3] = [
    -(KR * CB_SCALE + 0.5) as i64,
    -((1.0 - KR - KB) * CB_SCALE + 0.5) as i64,
    ((1.0 - KB) * CB_SCALE + 0.5) as i64,
];
const CR: [i64; 3] = [
    ((1.0 - KR) * CR_SCALE + 0.5) as i64,
    -((1.0 - KR - KB) * CR_SCALE + 0.5) as i64,
    -(KB * CR_SCALE + 0.5) as i64,
];

fn luma(r: i64, g: i64, b: i64) -> u8 {
    let [kr, kg, kb] = Y_RGB;
    let value = 16 + ((kr * r + kg * g + kb * b + (1 << 15)) >> 16);
    value.clamp(0, 255) as u8
}

/// A chroma sample from the sums of R', G' and B' over four pixels.
fn chroma(k: [i64; 3], r: i64, g: i64, b: i64) -> u8 {
    let [kr, kg, kb] = k;
    let value = 128 + ((kr * r + kg * g + kb * b + (1 << 17)) >> 18);
    value.clamp(0, 255) as u8
}

/// The peak signal-to-noise ratio of `b` against `a`, in decibels; infinite when they are equal.
pub fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let squared: u64 = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| u64::from(x.abs_diff(y)).pow(2))
        .sum();
    if squared == 0 {
        return f64::INFINITY;
    }
    let mean = squared as f64 / a.len().min(b.len()).max(1) as f64;
    10.0 * (255.0 * 255.0 / mean).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(width: u32, height: u32) -> Rgba {
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                pixels.extend([(x * 255 / width) as u8, (y * 255 / height) as u8, 128, 255]);
            }
        }
        Rgba {
            width,
            height,
            pixels,
        }
    }

    #[test]
    fn weights_sum_to_exactly_one() {
        for (from, to) in [(1920, 640), (640, 1920), (7, 3), (1, 5), (5, 1)] {
            for tap in taps(from, to) {
                assert_eq!(tap.weights.iter().sum::<u32>(), ONE, "{from} to {to}");
                assert!(tap.first + tap.weights.len() <= from);
            }
        }
    }

    #[test]
    fn one_colour_stays_that_colour_at_any_size() {
        let teal = Rgba::filled(1920, 1080, [11, 143, 140, 255]);
        assert_eq!(
            teal.resized(640, 360),
            Rgba::filled(640, 360, [11, 143, 140, 255])
        );
        assert_eq!(
            teal.resized(23, 1301),
            Rgba::filled(23, 1301, [11, 143, 140, 255])
        );
    }

    #[test]
    fn a_smooth_picture_survives_down_and_up() {
        let picture = gradient(640, 360);
        let back = picture.resized(320, 180).resized(640, 360);
        assert!(psnr(&picture.pixels, &back.pixels) > 40.0);
    }

    #[test]
    fn bt709_limited_range_codes() {
        let white = Rgba::filled(2, 2, [255, 255, 255, 255]).to_yuv420();
        assert_eq!((white.y[0], white.u[0], white.v[0]), (235, 128, 128));
        let black = Rgba::filled(2, 2, [0, 0, 0, 255]).to_yuv420();
        assert_eq!((black.y[0], black.u[0], black.v[0]), (16, 128, 128));
        let red = Rgba::filled(2, 2, [255, 0, 0, 255]).to_yuv420();
        assert_eq!((red.y[0], red.u[0], red.v[0]), (63, 102, 240));
        let blue = Rgba::filled(4, 2, [0, 0, 255, 255]).to_yuv420();
        assert_eq!((blue.y.len(), blue.u.len()), (8, 2));
        assert_eq!((blue.y[0], blue.u[0], blue.v[0]), (32, 240, 118));
    }

    #[test]
    fn fitting_letterboxes_on_black() {
        let square = Rgba::filled(100, 100, [255, 255, 255, 255]);
        let wide = square.fitted(160, 90);
        assert_eq!((wide.width, wide.height), (160, 90));
        assert_eq!(&wide.pixels[..4], &[0, 0, 0, 255]);
        let middle = (45 * 160 + 80) * 4;
        assert_eq!(&wide.pixels[middle..middle + 4], &[255, 255, 255, 255]);
    }
}
