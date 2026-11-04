//! Still pictures of a video: its thumbnails, and the storyboard the seek bar previews.
//!
//! Thumbnails are one frame at its display aspect, letterboxed into 16:9 at every width of
//! `routes::THUMBNAIL_WIDTHS`. The storyboard is one small tile every `interval`, laid out in
//! sheets of [`COLUMNS`]×[`ROWS`] tiles, row by row; tile `i` shows time `i × interval`. All are
//! JPEG, through the same encoder FilmCraft writes its stills with.

use image::ImageEncoder;
use image::codecs::jpeg::JpegEncoder;

use crate::Error;
use crate::picture::{Rgba, fit};

/// Storyboard tiles per sheet row.
pub const COLUMNS: u32 = 10;
/// Storyboard rows per sheet.
pub const ROWS: u32 = 10;
/// A storyboard tile's size: 16:9, as the cards and the player are.
pub const TILE: (u32, u32) = (160, 90);
/// The most tiles a storyboard holds; a longer video gets a longer interval.
pub const MAX_TILES: u64 = 400;

const QUALITY: u8 = 82;

/// `picture` as JPEG.
pub fn jpeg(picture: &Rgba) -> Result<Vec<u8>, Error> {
    let rgb: Vec<u8> = picture
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, _]| [r, g, b])
        .collect();
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, QUALITY)
        .write_image(
            &rgb,
            picture.width,
            picture.height,
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| Error::Encode(format!("jpeg: {e}")))?;
    Ok(out)
}

/// The thumbnail at `width`, 16:9, of `frame`, which shows at the `display` size.
pub fn thumbnail(frame: &Rgba, display: (u32, u32), width: u32) -> Rgba {
    let height = width * 9 / 16;
    frame.letterboxed(fit(display, width, height), width, height)
}

/// How many frames apart storyboard tiles are, for `frames` frames at `rate_num/rate_den` frames
/// per second: one second, or more so that there are at most [`MAX_TILES`].
pub fn tile_every(frames: u64, rate_num: u64, rate_den: u64) -> u64 {
    let second = (rate_num + rate_den / 2) / rate_den.max(1);
    second.max(frames.div_ceil(MAX_TILES)).max(1)
}

/// The sheets that `tiles` make, in order.
pub fn sheets(tiles: &[Rgba]) -> Vec<Rgba> {
    let (tile_w, tile_h) = TILE;
    let per_sheet = (COLUMNS * ROWS) as usize;
    tiles
        .chunks(per_sheet)
        .map(|chunk| {
            let rows = u32::try_from(chunk.len().div_ceil(COLUMNS as usize)).unwrap_or(ROWS);
            let mut sheet = Rgba::filled(tile_w * COLUMNS, tile_h * rows, [0, 0, 0, 255]);
            for (index, tile) in (0u32..).zip(chunk) {
                sheet.paste(tile, index % COLUMNS * tile_w, index / COLUMNS * tile_h);
            }
            sheet
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_are_a_second_apart_until_there_are_too_many() {
        assert_eq!(tile_every(30 * 60, 30, 1), 30);
        assert_eq!(tile_every(30 * 3600, 30, 1), 270);
        assert_eq!(tile_every(10, 30_000, 1001), 30);
    }

    #[test]
    fn sheets_hold_a_hundred_tiles_row_by_row() {
        let tiles: Vec<Rgba> = (0..103u32)
            .map(|i| Rgba::filled(TILE.0, TILE.1, [i as u8, 0, 0, 255]))
            .collect();
        let sheets = sheets(&tiles);
        assert_eq!(sheets.len(), 2);
        assert_eq!((sheets[0].width, sheets[0].height), (1600, 900));
        assert_eq!((sheets[1].width, sheets[1].height), (1600, 90));
        // tile 12 is row 1, column 2
        let at = ((90 * 1600) + 2 * 160) * 4;
        assert_eq!(sheets[0].pixels[at as usize], 12);
    }

    #[test]
    fn a_jpeg_is_a_jpeg() {
        let bytes = jpeg(&Rgba::filled(16, 9, [200, 30, 30, 255])).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
    }
}
