//! CLI image preparation. Kept separate from SDK-compatible raster generation.
use anyhow::{ensure, Result};
use image::{
    imageops::{resize, FilterType},
    RgbaImage,
};

const WIDTH: u32 = 576;
const MAX_RGBA: u64 = 128 * 1024 * 1024;

fn dimensions(w: u32, h: u32) -> Result<(u32, u32)> {
    ensure!(w > 0 && h > 0, "image dimensions must be positive");
    // Integer floor matches the sample's canvas.height conversion for normal
    // sizes. At least one row prevents extremely wide images becoming empty.
    let height = (u64::from(h) * u64::from(WIDTH) / u64::from(w)).max(1);
    ensure!(
        height * u64::from(WIDTH) * 4 <= MAX_RGBA,
        "576px resized image exceeds 128 MiB RGBA limit"
    );
    Ok((WIDTH, height as u32))
}

pub fn fit_width(image: RgbaImage, no_resize: bool) -> Result<RgbaImage> {
    ensure!(
        image.width() > 0 && image.height() > 0,
        "image dimensions must be positive"
    );
    if no_resize || image.width() == WIDTH {
        return Ok(image);
    }
    let (w, h) = dimensions(image.width(), image.height())?;
    // imageops::resize expects premultiplied alpha for correct interpolation.
    // Use u8 premultiplication to keep bounded intermediate allocations.
    let mut premultiplied = image;
    for p in premultiplied.pixels_mut() {
        let a = u32::from(p[3]);
        for c in &mut p.0[..3] {
            *c = ((u32::from(*c) * a + 127) / 255) as u8;
        }
    }
    let mut result = resize(&premultiplied, w, h, FilterType::Triangle);
    for p in result.pixels_mut() {
        let a = u32::from(p[3]);
        for c in &mut p.0[..3] {
            *c = if a == 0 {
                0
            } else {
                ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8
            };
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    #[test]
    fn aspect_and_limits() -> Result<()> {
        assert_eq!(dimensions(288, 144)?, (576, 288));
        assert_eq!(dimensions(1152, 1001)?, (576, 500));
        assert_eq!(dimensions(16384, 1)?, (576, 1));
        assert!(dimensions(1, 16384).is_err());
        assert!(dimensions(0, 1).is_err());
        Ok(())
    }
    #[test]
    fn fit_and_bypass() -> Result<()> {
        let image = RgbaImage::from_pixel(8, 8, Rgba([255, 255, 255, 255]));
        let fitted = fit_width(image.clone(), false)?;
        assert_eq!(fitted.dimensions(), (576, 576));
        assert!(fitted.pixels().all(|p| p.0 == [255; 4]));
        assert_eq!(fit_width(image.clone(), true)?, image);
        let exact = RgbaImage::from_pixel(576, 2, Rgba([73, 45, 12, 127]));
        assert_eq!(fit_width(exact.clone(), false)?, exact);
        Ok(())
    }
    #[test]
    fn transparent_edges_do_not_leak_hidden_rgb() -> Result<()> {
        let mut image = RgbaImage::new(2, 1);
        image.put_pixel(0, 0, Rgba([255, 0, 0, 0]));
        image.put_pixel(1, 0, Rgba([255, 255, 255, 255]));
        let fitted = fit_width(image, false)?;
        for p in fitted.pixels().filter(|p| p[3] > 0) {
            assert_eq!(p[0], p[1]);
            assert_eq!(p[1], p[2]);
        }
        Ok(())
    }
}
