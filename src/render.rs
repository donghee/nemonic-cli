//! Native reconstruction of SDK 0.9.1's image pipeline.
//! This module has no WASM runtime or vendor binary dependency.
use anyhow::{ensure, Result};

pub fn build(width: u32, height: u32, rgba: &[u8], copies: u8, dither: bool) -> Result<Vec<u8>> {
    ensure!(width > 0 && height > 0, "image dimensions must be positive");
    ensure!(copies > 0, "copies must be 1..255");
    let expected = u64::from(width) * u64::from(height);
    let expected = expected
        .checked_mul(4)
        .ok_or_else(|| anyhow::anyhow!("image dimensions overflow"))?;
    ensure!(expected == rgba.len() as u64, "RGBA length mismatch");
    ensure!(
        expected <= 128 * 1024 * 1024,
        "image exceeds 128 MiB RGBA limit"
    );
    let w = width as usize;
    let h = height.min(2000) as usize;
    let stride = w.div_ceil(8);
    ensure!(
        stride <= u16::MAX as usize,
        "raster width exceeds 65535 bytes"
    );
    let mut raster = vec![0u8; stride * h];
    if dither {
        dither_raster(w, h, rgba, stride, &mut raster);
    } else {
        for y in 0..h {
            for x in 0..w {
                let p = &rgba[(y * w + x) * 4..][..4];
                // HSL lightness (not weighted luma). Keep f32 evaluation order:
                // alpha compositing and the exact 128 boundary affect output bits.
                let mut rgb = [p[0] as f32, p[1] as f32, p[2] as f32];
                if p[3] != 255 {
                    let a = p[3] as f32 / 255.0;
                    let white = (1.0 - a) * 255.0;
                    for c in &mut rgb {
                        *c = a * *c + white;
                    }
                }
                let light = (rgb[0].max(rgb[1]).max(rgb[2]) + rgb[0].min(rgb[1]).min(rgb[2])) * 0.5;
                if light < 128.0 {
                    raster[y * stride + x / 8] |= 0x80 >> (x % 8);
                }
            }
        }
    }
    let mut out = Vec::with_capacity(raster.len() + 17);
    out.extend_from_slice(&[0x02, 0x1b, 0x40, 0x1b, 0x43, copies, 0x1d, 0x76, 0x30, 0x00]);
    out.extend_from_slice(&(stride as u16).to_le_bytes());
    out.extend_from_slice(&(h as u16).to_le_bytes());
    out.extend_from_slice(&raster);
    out.extend_from_slice(&[0x1b, 0x50, 0x03]);
    Ok(out)
}

fn dither_raster(w: usize, h: usize, rgba: &[u8], stride: usize, raster: &mut [u8]) {
    // The SDK sharpens uncomposited RGB with a 3x3 kernel before flattening
    // alpha. Border pixels are copied unchanged. Alpha is the center alpha.
    let mut pixels: Vec<[u8; 4]> = rgba[..w * h * 4]
        .chunks_exact(4)
        .map(|p| [p[0], p[1], p[2], p[3]])
        .collect();
    if w >= 3 && h >= 3 {
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let i = y * w + x;
                for c in 0..3 {
                    let mut neighbors = 0i32;
                    for yy in y - 1..=y + 1 {
                        for xx in x - 1..=x + 1 {
                            if xx != x || yy != y {
                                neighbors += rgba[(yy * w + xx) * 4 + c] as i32;
                            }
                        }
                    }
                    let v = (13 * rgba[i * 4 + c] as i32 - neighbors) / 5;
                    pixels[i][c] = v.clamp(0, 255) as u8;
                }
            }
        }
    }
    let mut gray: Vec<i32> = pixels
        .iter()
        .map(|p| {
            let a = p[3] as u32;
            // SDK's packed ARGB path uses integer alpha blending and truncation.
            let blend = |c: u8| (u32::from(c) * a + (255 - a) * 255) / 255;
            let r = blend(p[0]) as f32;
            let g = blend(p[1]) as f32;
            let b = blend(p[2]) as f32;
            // Preserve WASM f32 operation order (no fused multiply-add).
            (b * 0.114_f32 + (g * 0.587_f32 + r * 0.299_f32)) as i32
        })
        .collect();
    // Three-row Sierra error diffusion, denominator 32.
    const DIFFUSE: [(isize, usize, i32); 10] = [
        (1, 0, 5),
        (2, 0, 3),
        (-2, 1, 2),
        (-1, 1, 4),
        (0, 1, 5),
        (1, 1, 4),
        (2, 1, 2),
        (-1, 2, 2),
        (0, 2, 3),
        (1, 2, 2),
    ];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let value = gray[i];
            let error = if value <= 127 {
                raster[y * stride + x / 8] |= 0x80 >> (x % 8);
                value.max(0)
            } else {
                value.min(255) - 255
            };
            for (dx, dy, weight) in DIFFUSE {
                let xx = x as isize + dx;
                if xx >= 0 && xx < w as isize && y + dy < h {
                    // Signed division truncates toward zero, not floor. +16 is
                    // also used for negative errors in the original WASM.
                    gray[(y + dy) * w + xx as usize] += (error * weight + 16) / 32;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn threshold_and_alpha() -> Result<()> {
        let rgba = [
            127, 127, 127, 255, 128, 128, 128, 255, 0, 0, 0, 0, 0, 0, 0, 127, 0, 0, 0, 128, 255, 0,
            0, 255,
        ];
        let cmd = build(6, 1, &rgba, 1, false)?;
        assert_eq!(cmd[14], 0x9c); // f32 alpha=127 produces 127.99999, hence black
        assert_eq!(&cmd[..10], &[2, 27, 64, 27, 67, 1, 29, 118, 48, 0]);
        assert_eq!(&cmd[15..], &[27, 80, 3]);
        Ok(())
    }
    #[test]
    fn padding_and_copies() -> Result<()> {
        let cmd = build(9, 2, &[0; 72], 255, false)?; // fully transparent => white
        assert_eq!(&cmd[10..14], &[2, 0, 2, 0]);
        assert_eq!(&cmd[14..18], &[0, 0, 0, 0]);
        assert_eq!(cmd[5], 255);
        let mut rgba = [0; 72];
        for p in rgba.chunks_exact_mut(4) {
            p[3] = 255;
        }
        let cmd = build(9, 2, &rgba, 1, false)?;
        assert_eq!(&cmd[14..18], &[255, 128, 255, 128]);
        Ok(())
    }
    #[test]
    fn overflow_is_an_error_not_panic() {
        assert!(build(u32::MAX, u32::MAX, &[], 1, true).is_err());
        assert!(build(65535 * 8 + 1, 1, &[], 1, false).is_err());
    }
}
