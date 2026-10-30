//! Small, bounded library images, decoded without GL or the preview's blur pass.
use anyhow::{Result, ensure};
use image::{ImageReader, imageops::FilterType};
use std::{
    fs::File,
    io::{Cursor, Read},
    path::Path,
};

pub struct CoverImage {
    pub size: [usize; 2],
    pub rgba: Vec<u8>,
}
pub fn load_project_cover(path: &Path) -> Result<CoverImage> {
    let image = decode_image(path)?
        .resize_to_fill(168, 104, FilterType::Triangle)
        .into_rgba8();
    Ok(CoverImage {
        size: [image.width() as usize, image.height() as usize],
        rgba: image.into_raw(),
    })
}
pub fn load_texture_thumbnail(path: &Path) -> Result<CoverImage> {
    let image = decode_image(path)?
        .resize(160, 96, FilterType::Triangle)
        .into_rgba8();
    Ok(CoverImage {
        size: [image.width() as usize, image.height() as usize],
        rgba: image.into_raw(),
    })
}
fn decode_image(path: &Path) -> Result<image::DynamicImage> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 32 * 1024 * 1024, "Cover exceeds 32 MiB");
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader.decode()?)
}

/// Validate a selected audio file through the same decoder as actual playback.
pub fn audio_file_length(path: &Path) -> Result<f64> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(128 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= 128 * 1024 * 1024,
        "Audio must be nonempty and at most 128 MiB"
    );
    let clip = sasa::AudioClip::new(bytes)?;
    let duration = clip.length();
    ensure!(
        duration.is_finite() && duration > 0.,
        "Audio must contain playable samples"
    );
    Ok(duration)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn texture_thumbnails_keep_aspect_alpha_and_decode_gif_first_frames() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("wide.png");
        image::RgbaImage::from_pixel(200, 10, image::Rgba([200, 80, 10, 127]))
            .save(&path)
            .unwrap();
        let thumbnail = load_texture_thumbnail(&path).unwrap();
        assert_eq!(thumbnail.size, [160, 8]);
        assert!(
            thumbnail
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [200, 80, 10, 127])
        );
        let path = root.path().join("animation.gif");
        image::RgbaImage::from_pixel(10, 20, image::Rgba([100, 50, 70, 255]))
            .save_with_format(&path, image::ImageFormat::Gif)
            .unwrap();
        let thumbnail = load_texture_thumbnail(&path).unwrap();
        assert_eq!(thumbnail.size, [48, 96]);
        assert!(
            thumbnail
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [100, 50, 70, 255])
        );
        std::fs::write(&path, b"broken image").unwrap();
        assert!(load_texture_thumbnail(&path).is_err());
    }
    #[test]
    fn cover_decode_is_small_preserves_pixels_and_handles_missing_or_broken_artwork() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("artwork.data");
        let image = image::RgbaImage::from_pixel(800, 200, image::Rgba([200, 90, 20, 255]));
        image
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        let cover = load_project_cover(&path).unwrap();
        assert_eq!(cover.size, [168, 104]);
        assert!(
            cover
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [200, 90, 20, 255])
        );
        std::fs::write(&path, b"not an image").unwrap();
        assert!(load_project_cover(&path).is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(load_project_cover(&path).is_err());
    }
}
