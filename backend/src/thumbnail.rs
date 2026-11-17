use std::path::Path;
use image::ImageFormat;

pub const THUMBNAIL_MAX_EDGE: u32 = 320;


pub fn generate(src: &Path, dest: &Path) -> anyhow::Result<()> {
    let image = image::ImageReader::open(src)?
        .with_guessed_format()?
        .decode()?;
    let thumb = image.thumbnail(THUMBNAIL_MAX_EDGE, THUMBNAIL_MAX_EDGE);
    thumb.save_with_format(dest, ImageFormat::Png)?;
    Ok(())
}
