//! A channel's photo: any JPEG, PNG or WebP turned upright, cut square at its centre, and
//! written as JPEG at every size of `routes::PHOTO_SIZES`.
//!
//! The server runs [`photo`] in a child process with resource limits, as it does transcodes;
//! the decoder's own limits also refuse, before any pixel is decoded, a picture larger than
//! [`MAX_SIDE`] or one that would take more than [`MAX_DECODED`] bytes. Transparent pixels are
//! laid on white, since JPEG has no transparency.

use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use youcraft_api::routes::{PHOTO_SIZES, photo_file};

use crate::Error;
use crate::picture::Rgba;
use crate::stills::jpeg;

/// The longest side of a photo the decoder accepts, in pixels.
pub const MAX_SIDE: u32 = 12_000;
/// The most bytes the decoder may allocate for one photo.
pub const MAX_DECODED: u64 = 512 << 20;

/// Make the photo in `bytes` into its JPEG files in the directory `output`.
pub fn photo(bytes: &[u8], output: &Path) -> Result<(), Error> {
    let square = square(&decoded(bytes)?);
    std::fs::create_dir_all(output).map_err(|e| Error::io(output, &e))?;
    for size in PHOTO_SIZES {
        let file = output.join(photo_file(size));
        let made = jpeg(&square.resized(size, size))?;
        std::fs::write(&file, made).map_err(|e| Error::io(&file, &e))?;
    }
    Ok(())
}

/// The picture in `bytes`, upright as its EXIF orientation says, on white.
pub fn decoded(bytes: &[u8]) -> Result<Rgba, Error> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| Error::Unsupported(format!("photo: {e}")))?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP)
    ) {
        return Err(Error::Unsupported(
            "photo: not a JPEG, PNG or WebP file".to_owned(),
        ));
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_DECODED);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| Error::Decode(format!("photo: {e}")))?;
    let orientation = decoder.orientation().ok();
    let mut picture =
        DynamicImage::from_decoder(decoder).map_err(|e| Error::Decode(format!("photo: {e}")))?;
    if let Some(orientation) = orientation {
        picture.apply_orientation(orientation);
    }
    let rgba = picture.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut pixels = rgba.into_raw();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        for channel in pixel.iter_mut().take(3) {
            // straight alpha over white, rounded
            *channel = ((u32::from(*channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
        }
        pixel[3] = 255;
    }
    Ok(Rgba {
        width,
        height,
        pixels,
    })
}

/// The largest square at the centre of `picture`.
pub fn square(picture: &Rgba) -> Rgba {
    let side = picture.width.min(picture.height);
    let (x, y) = ((picture.width - side) / 2, (picture.height - side) / 2);
    let stride = picture.width as usize * 4;
    let mut pixels = Vec::with_capacity(side as usize * side as usize * 4);
    for row in picture
        .pixels
        .chunks_exact(stride.max(1))
        .skip(y as usize)
        .take(side as usize)
    {
        let start = x as usize * 4;
        pixels.extend_from_slice(
            row.get(start..start + side as usize * 4)
                .unwrap_or_default(),
        );
    }
    Rgba {
        width: side,
        height: side,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `width`×`height` PNG whose left half is red and right half blue, half transparent at
    /// the top row.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let picture = image::RgbaImage::from_fn(width, height, |x, y| {
            let alpha = if y == 0 { 0 } else { 255 };
            if x < width / 2 {
                image::Rgba([255, 0, 0, alpha])
            } else {
                image::Rgba([0, 0, 255, alpha])
            }
        });
        let mut out = Cursor::new(Vec::new());
        picture.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn a_photo_is_cut_square_at_its_centre_at_every_size() {
        let wide = decoded(&png(300, 100)).unwrap();
        assert_eq!((wide.width, wide.height), (300, 100));
        assert_eq!(
            &wide.pixels[..4],
            &[255, 255, 255, 255],
            "transparent on white"
        );
        let cut = square(&wide);
        assert_eq!((cut.width, cut.height), (100, 100));
        let row = &cut.pixels[100 * 4 * 50..100 * 4 * 51];
        assert_eq!(
            &row[..4],
            &[255, 0, 0, 255],
            "the left of the centre is red"
        );
        assert_eq!(&row[99 * 4..], &[0, 0, 255, 255], "the right is blue");
        let dir = std::env::temp_dir().join(format!("youcraft-photo-{}", std::process::id()));
        photo(&png(64, 128), &dir).unwrap();
        for size in PHOTO_SIZES {
            let file = std::fs::read(dir.join(photo_file(size))).unwrap();
            let made = image::load_from_memory(&file).unwrap();
            assert_eq!((made.width(), made.height()), (size, size));
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn what_is_not_a_photo_is_refused() {
        assert!(matches!(
            decoded(b"GIF89a not really"),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            decoded(b"\x89PNG\r\n\x1a\nbroken"),
            Err(Error::Decode(_))
        ));
    }
}
