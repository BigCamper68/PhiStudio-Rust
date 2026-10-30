//! Phira LoadingScene's RGB fastblur 0.1.1 pipeline, including its radius.
//! Decode bounds and GPU resizing belong to the editor, outside the engine.
use anyhow::Result;
use image::{DynamicImage, ImageReader, RgbImage};
use std::io::Cursor;

pub(crate) fn decode(bytes: Vec<u8>) -> Result<(DynamicImage, RgbImage)> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let illustration = reader.decode()?;
    let (width, height) = (illustration.width(), illustration.height());
    let mut pixels: Vec<[u8; 3]> = illustration.to_rgb8().pixels().map(|p| p.0).collect();
    fastblur::gaussian_blur(&mut pixels, width as usize, height as usize, 50.);
    let background = RgbImage::from_raw(width, height, pixels.into_flattened()).unwrap();
    Ok((illustration, background))
}
