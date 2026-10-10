//! Mask-guided editing support: mask validation and the deterministic
//! source/result composite.
//!
//! Mask convention: white (255) is editable, black (0) preserves the source,
//! intermediate values are transition weights. Qwen-Image-2.1 has no native
//! mask input; the model may regenerate the full frame and this module limits
//! which of its pixels are published.

use std::fmt;

use image::{GrayImage, Rgba, RgbaImage};

/// Largest accepted feather radius, in source pixels.
pub const MAX_FEATHER_PX: u32 = 256;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MaskedEditOptions {
    /// Feather radius in source-image pixels; 0 means a hard mask.
    pub feather_px: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskedEditError {
    /// The mask has no editable pixels.
    EmptyMask,
    /// Source, generated image and mask dimensions differ.
    DimensionMismatch {
        what: &'static str,
        expected: (u32, u32),
        actual: (u32, u32),
    },
    /// Feather radius above [`MAX_FEATHER_PX`].
    InvalidFeather(u32),
    /// Decoding, encoding or other image-processing failure.
    Image(String),
}

impl fmt::Display for MaskedEditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyMask => write!(f, "the mask has no editable area"),
            Self::DimensionMismatch {
                what,
                expected,
                actual,
            } => write!(
                f,
                "{what} is {}x{} but the source is {}x{}",
                actual.0, actual.1, expected.0, expected.1
            ),
            Self::InvalidFeather(px) => write!(
                f,
                "feather radius {px}px exceeds the maximum of {MAX_FEATHER_PX}px"
            ),
            Self::Image(msg) => write!(f, "image processing failed: {msg}"),
        }
    }
}

impl std::error::Error for MaskedEditError {}

impl From<image::ImageError> for MaskedEditError {
    fn from(e: image::ImageError) -> Self {
        Self::Image(e.to_string())
    }
}

/// Checks that the mask matches the source and contains editable pixels.
pub fn validate_mask(source: &RgbaImage, mask: &GrayImage) -> Result<(), MaskedEditError> {
    if source.dimensions() != mask.dimensions() {
        return Err(MaskedEditError::DimensionMismatch {
            what: "mask",
            expected: source.dimensions(),
            actual: mask.dimensions(),
        });
    }
    if mask.as_raw().iter().all(|&v| v == 0) {
        return Err(MaskedEditError::EmptyMask);
    }
    Ok(())
}

/// Per-pixel blend weight (0.0..=1.0) of the generated image.
///
/// Feathering is inward-only: the weight ramps up with the distance from the
/// nearest zero-mask pixel, so zero-mask pixels never change and the editable
/// region never grows. The image border is not treated as an edge.
fn blend_weights(mask: &GrayImage, feather_px: u32) -> Vec<f32> {
    let (w, h) = mask.dimensions();
    let (w, h) = (w as usize, h as usize);
    let raw = mask.as_raw();
    let base = |v: u8| v as f32 / 255.0;
    if feather_px == 0 {
        return raw.iter().map(|&v| base(v)).collect();
    }

    // Two-pass chamfer distance to the nearest zero-mask pixel.
    const INF: f32 = f32::MAX / 4.0;
    const DIAG: f32 = std::f32::consts::SQRT_2;
    let mut dist: Vec<f32> = raw
        .iter()
        .map(|&v| if v == 0 { 0.0 } else { INF })
        .collect();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let mut d = dist[i];
            if x > 0 {
                d = d.min(dist[i - 1] + 1.0);
            }
            if y > 0 {
                d = d.min(dist[i - w] + 1.0);
                if x > 0 {
                    d = d.min(dist[i - w - 1] + DIAG);
                }
                if x + 1 < w {
                    d = d.min(dist[i - w + 1] + DIAG);
                }
            }
            dist[i] = d;
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            let mut d = dist[i];
            if x + 1 < w {
                d = d.min(dist[i + 1] + 1.0);
            }
            if y + 1 < h {
                d = d.min(dist[i + w] + 1.0);
                if x + 1 < w {
                    d = d.min(dist[i + w + 1] + DIAG);
                }
                if x > 0 {
                    d = d.min(dist[i + w - 1] + DIAG);
                }
            }
            dist[i] = d;
        }
    }

    let f = feather_px as f32;
    raw.iter()
        .zip(&dist)
        .map(|(&v, &d)| {
            if v == 0 {
                0.0
            } else {
                base(v) * (d / f).min(1.0)
            }
        })
        .collect()
}

fn blend(src: &Rgba<u8>, gen: &Rgba<u8>, t: f32) -> Rgba<u8> {
    if t <= 0.0 {
        return *src;
    }
    if t >= 1.0 {
        return *gen;
    }
    let sa = src[3] as f32 / 255.0;
    let ga = gen[3] as f32 / 255.0;
    let oa = sa * (1.0 - t) + ga * t;
    if oa <= 0.0 {
        return Rgba([0, 0, 0, 0]);
    }
    let mut out = [0u8; 4];
    for c in 0..3 {
        // Premultiplied interpolation avoids halos from the hidden colour of
        // transparent pixels.
        let v = (src[c] as f32 * sa * (1.0 - t) + gen[c] as f32 * ga * t) / oa;
        out[c] = v.round().clamp(0.0, 255.0) as u8;
    }
    out[3] = (oa * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgba(out)
}

/// Publishes `generated` only inside `mask`; everything else is copied from
/// `source` byte for byte. All three images must share dimensions.
pub fn composite(
    source: &RgbaImage,
    generated: &RgbaImage,
    mask: &GrayImage,
    options: &MaskedEditOptions,
) -> Result<RgbaImage, MaskedEditError> {
    if options.feather_px > MAX_FEATHER_PX {
        return Err(MaskedEditError::InvalidFeather(options.feather_px));
    }
    validate_mask(source, mask)?;
    if generated.dimensions() != source.dimensions() {
        return Err(MaskedEditError::DimensionMismatch {
            what: "generated image",
            expected: source.dimensions(),
            actual: generated.dimensions(),
        });
    }
    let weights = blend_weights(mask, options.feather_px);
    let (w, h) = source.dimensions();
    let mut out = RgbaImage::new(w, h);
    for (i, (o, (s, g))) in out
        .pixels_mut()
        .zip(source.pixels().zip(generated.pixels()))
        .enumerate()
    {
        *o = blend(s, g, weights[i]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    const SRC: Rgba<u8> = Rgba([10, 20, 30, 255]);
    const GEN: Rgba<u8> = Rgba([200, 180, 160, 255]);

    fn solid(w: u32, h: u32, p: Rgba<u8>) -> RgbaImage {
        RgbaImage::from_pixel(w, h, p)
    }

    fn hard() -> MaskedEditOptions {
        MaskedEditOptions { feather_px: 0 }
    }

    #[test]
    fn black_mask_is_rejected_as_empty() {
        let s = solid(4, 4, SRC);
        let m = GrayImage::new(4, 4);
        assert_eq!(validate_mask(&s, &m), Err(MaskedEditError::EmptyMask));
        assert_eq!(
            composite(&s, &solid(4, 4, GEN), &m, &hard()),
            Err(MaskedEditError::EmptyMask)
        );
    }

    #[test]
    fn white_mask_returns_generated() {
        let m = GrayImage::from_pixel(4, 4, Luma([255]));
        let out = composite(&solid(4, 4, SRC), &solid(4, 4, GEN), &m, &hard()).unwrap();
        assert!(out.pixels().all(|p| *p == GEN));
        let out = composite(
            &solid(4, 4, SRC),
            &solid(4, 4, GEN),
            &m,
            &MaskedEditOptions { feather_px: 8 },
        )
        .unwrap();
        assert!(
            out.pixels().all(|p| *p == GEN),
            "full mask has no inner edge"
        );
    }

    #[test]
    fn outside_mask_is_exact_source() {
        let mut src = solid(8, 8, SRC);
        src.put_pixel(0, 0, Rgba([1, 2, 3, 77]));
        let mut m = GrayImage::new(8, 8);
        for y in 2..6 {
            for x in 2..6 {
                m.put_pixel(x, y, Luma([255]));
            }
        }
        for feather in [0, 1, 3, 100] {
            let out = composite(
                &src,
                &solid(8, 8, GEN),
                &m,
                &MaskedEditOptions {
                    feather_px: feather,
                },
            )
            .unwrap();
            for (x, y, p) in out.enumerate_pixels() {
                if m.get_pixel(x, y)[0] == 0 {
                    assert_eq!(p, src.get_pixel(x, y), "feather {feather} at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn feather_ramps_inward_only() {
        let mut m = GrayImage::new(16, 1);
        for x in 4..16 {
            m.put_pixel(x, 0, Luma([255]));
        }
        let out = composite(
            &solid(16, 1, Rgba([0, 0, 0, 255])),
            &solid(16, 1, Rgba([200, 200, 200, 255])),
            &m,
            &MaskedEditOptions { feather_px: 4 },
        )
        .unwrap();
        assert_eq!(out.get_pixel(3, 0)[0], 0);
        assert_eq!(out.get_pixel(4, 0)[0], 50); // d=1 -> 1/4
        assert_eq!(out.get_pixel(5, 0)[0], 100);
        assert_eq!(out.get_pixel(7, 0)[0], 200); // d>=4 -> full
        assert_eq!(out.get_pixel(15, 0)[0], 200);
    }

    #[test]
    fn mid_gray_blends() {
        let m = GrayImage::from_pixel(2, 2, Luma([128]));
        let out = composite(&solid(2, 2, SRC), &solid(2, 2, GEN), &m, &hard()).unwrap();
        let p = out.get_pixel(0, 0);
        let t = 128.0 / 255.0;
        for c in 0..3 {
            let want = SRC[c] as f32 * (1.0 - t) + GEN[c] as f32 * t;
            assert!(
                (p[c] as f32 - want).abs() <= 1.0,
                "channel {c}: {} vs {want}",
                p[c]
            );
        }
        assert_eq!(p[3], 255);
    }

    #[test]
    fn transparent_pixels_do_not_halo() {
        // Generated pixel is fully transparent with black hidden colour; the
        // visible source colour must not darken.
        let src = solid(2, 1, Rgba([200, 100, 50, 255]));
        let gen = solid(2, 1, Rgba([0, 0, 0, 0]));
        let m = GrayImage::from_pixel(2, 1, Luma([128]));
        let out = composite(&src, &gen, &m, &hard()).unwrap();
        let p = out.get_pixel(0, 0);
        assert_eq!(&p.0[..3], &[200, 100, 50]);
        assert!((p[3] as i32 - 127).abs() <= 1);

        let both = composite(
            &solid(1, 1, Rgba([9, 9, 9, 0])),
            &solid(1, 1, Rgba([7, 7, 7, 0])),
            &GrayImage::from_pixel(1, 1, Luma([128])),
            &hard(),
        )
        .unwrap();
        assert_eq!(both.get_pixel(0, 0)[3], 0);
    }

    #[test]
    fn mismatched_dimensions_error() {
        let m = GrayImage::from_pixel(4, 4, Luma([255]));
        assert!(matches!(
            composite(&solid(4, 4, SRC), &solid(5, 4, GEN), &m, &hard()),
            Err(MaskedEditError::DimensionMismatch {
                what: "generated image",
                ..
            })
        ));
        assert!(matches!(
            validate_mask(&solid(5, 4, SRC), &m),
            Err(MaskedEditError::DimensionMismatch { what: "mask", .. })
        ));
    }

    #[test]
    fn oversized_feather_is_rejected() {
        let m = GrayImage::from_pixel(2, 2, Luma([255]));
        let err = composite(
            &solid(2, 2, SRC),
            &solid(2, 2, GEN),
            &m,
            &MaskedEditOptions {
                feather_px: MAX_FEATHER_PX + 1,
            },
        )
        .unwrap_err();
        assert_eq!(err, MaskedEditError::InvalidFeather(MAX_FEATHER_PX + 1));
    }
}
