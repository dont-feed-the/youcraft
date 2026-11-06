//! The skim sheet: the frames a video's thumbnail shows as a mouse moves across it. The
//! transcoder makes it and the page reads it; both agree on its layout here.
//!
//! The video is cut into [`TILES`] equal parts, and tile `i` shows the frame at the middle of part
//! `i`. The tiles are [`TILE`] pixels each, [`COLUMNS`] to a row, in one JPEG of [`ROWS`] rows. A
//! pointer `f` of the way across the thumbnail is over part ⌊f × TILES⌋ and sees its middle, so
//! moving from one edge to the other shows the whole video once, evenly, whatever its length.

/// Tiles per row.
pub const COLUMNS: u32 = 10;
/// Rows of tiles.
pub const ROWS: u32 = 5;
/// Tiles: the equal parts the video is cut into.
pub const TILES: u32 = COLUMNS * ROWS;
/// A tile's size in pixels: 16:9, as thumbnails are.
pub const TILE: (u32, u32) = (320, 180);

/// The frame that tile `tile` shows, of a video of `frames` frames that all last as long: the one
/// showing at the middle of part `tile`, ⌊(2 × tile + 1) × frames / (2 × TILES)⌋, counted exactly.
pub fn frame_of(tile: u32, frames: u64) -> u64 {
    let halves = 2 * u128::from(tile) + 1;
    let frame = halves * u128::from(frames) / (2 * u128::from(TILES));
    u64::try_from(frame).unwrap_or(u64::MAX)
}

/// The tile under a pointer `fraction` of the way across a thumbnail, from 0 at its left edge,
/// the start of the video in every language, to 1 at its right: ⌊fraction × TILES⌋, the last
/// at 1 and beyond, the first below 0 and for NaN.
pub fn tile_under(fraction: f64) -> u32 {
    // A float-to-integer `as` saturates, and takes NaN to 0.
    ((fraction * f64::from(TILES)).floor() as u32).min(TILES - 1)
}

/// The column and row of tile `tile` in the sheet, counted from its top left.
pub fn place(tile: u32) -> (u32, u32) {
    (tile % COLUMNS, tile / COLUMNS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_tile_shows_the_middle_of_its_part() {
        // 140 frames in 50 parts of 2.8 frames: part 0 is [0, 2.8), its middle in frame 1.
        assert_eq!(frame_of(0, 140), 1);
        assert_eq!(frame_of(1, 140), 4);
        assert_eq!(frame_of(49, 140), 138);
        // A video of fewer frames than tiles shows each of them in order, as evenly as 50
        // tiles share 7 frames.
        let short: Vec<u64> = (0..TILES).map(|tile| frame_of(tile, 7)).collect();
        assert!(short.windows(2).all(|pair| pair[0] <= pair[1]));
        for frame in 0..7 {
            let shown = short.iter().filter(|&&shown| shown == frame).count();
            assert!(
                shown == 7 || shown == 8,
                "frame {frame} is shown {shown} times"
            );
        }
        // Every tile shows a frame of the video, however long.
        for frames in [1, 2, 49, 50, 51, u64::MAX] {
            assert!((0..TILES).all(|tile| frame_of(tile, frames) < frames));
        }
    }

    #[test]
    fn the_pointer_is_over_the_part_it_is_in() {
        assert_eq!(tile_under(0.0), 0);
        assert_eq!(tile_under(0.0199), 0);
        assert_eq!(tile_under(0.02), 1);
        assert_eq!(tile_under(0.5), 25);
        assert_eq!(tile_under(1.0), TILES - 1);
        assert_eq!(tile_under(7.0), TILES - 1);
        assert_eq!(tile_under(-1.0), 0);
        assert_eq!(tile_under(f64::NAN), 0);
        // The middle of each part is over that part, and its tile shows the frame there.
        for tile in 0..TILES {
            let middle = (f64::from(tile) + 0.5) / f64::from(TILES);
            assert_eq!(tile_under(middle), tile);
        }
    }

    #[test]
    fn tiles_go_row_by_row() {
        assert_eq!(place(0), (0, 0));
        assert_eq!(place(12), (2, 1));
        assert_eq!(place(TILES - 1), (COLUMNS - 1, ROWS - 1));
    }
}
