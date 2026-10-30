use std::path::Path;

use image::ImageFormat;

/// Longest-edge size (px) for generated thumbnails.
pub const THUMBNAIL_MAX_EDGE: u32 = 320;

/// Decode the image at `src`, downscale it preserving aspect ratio, and write a
/// PNG thumbnail to `dest`. CPU-bound — call via `spawn_blocking`.
pub fn generate(src: &Path, dest: &Path) -> anyhow::Result<()> {
    let image = image::ImageReader::open(src)?
        .with_guessed_format()?
        .decode()?;
    // `thumbnail` only ever downscales and is fast; PNG keeps any transparency.
    let thumb = image.thumbnail(THUMBNAIL_MAX_EDGE, THUMBNAIL_MAX_EDGE);
    thumb.save_with_format(dest, ImageFormat::Png)?;
    Ok(())
}
