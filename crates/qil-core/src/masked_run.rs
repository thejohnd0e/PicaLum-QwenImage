//! Mask-guided edit run: marks the editable region on a copy of the source,
//! runs the normal Qwen edit pipeline, resamples the raw result to the source
//! canvas and publishes only the final composite.
//!
//! Guidance strategy `marked-reference-v1` (chosen by the prototype in
//! `bench/mask_proto`): the model sees the source with a red outline and light
//! red tint over the editable region and is told to edit only inside it. There
//! is no native mask input; [`crate::masked_edit::composite`] is what
//! guarantees pixels outside the mask are untouched.

use std::path::{Path, PathBuf};
use std::time::Instant;

use image::imageops::FilterType;
use image::GenericImageView;
use image::{GrayImage, Rgb, RgbImage, RgbaImage};
use serde_json::json;

use crate::config::DIT;
use crate::generate::{
    preset, remove_temp_file, run_error_text, run_with_control, should_save_json,
    unique_output_path, GenRequest, GenResult, GenerateOptions,
};
use crate::masked_edit::{composite, validate_mask, MaskedEditError, MaskedEditOptions};
use crate::png_metadata::{embed_generation_metadata, GenerationMetadata};
use crate::run_control::RunError;

pub const STRATEGY_ID: &str = "marked-reference-v1";
pub const MODE: &str = "masked-edit";
/// Largest accepted source, in pixels.
pub const MAX_PIXELS: u64 = 100_000_000;
/// Largest accepted instruction, in characters.
pub const MAX_INSTRUCTION_CHARS: usize = 4000;
/// The marked source takes one of the engine's reference slots.
pub const MAX_REFERENCES: usize = crate::generate::MAX_REFS - 1;
const TINT: f32 = 0.25;

#[derive(Debug, Clone)]
pub struct MaskedEditRequest {
    pub source_path: PathBuf,
    /// PNG-encoded mask, source-sized, white = editable.
    pub mask_png: Vec<u8>,
    pub instruction: String,
    pub preset: String,
    pub seed: Option<u64>,
    pub feather_px: u32,
    /// Optional reference photos for the content to place in the area; they are
    /// sent after the marked source, so the first one is `<image2>`.
    pub references: Vec<PathBuf>,
}

/// Decodes an image as it should be seen: phone photos are often stored
/// sideways with an EXIF orientation tag, which `image::open` ignores.
pub fn open_upright(path: &Path) -> Result<image::DynamicImage, String> {
    let err = |e: &dyn std::fmt::Display| format!("Cannot open the image: {e}");
    let mut decoder = image::ImageReader::open(path)
        .and_then(|r| r.with_guessed_format())
        .map_err(|e| err(&e))?
        .into_decoder()
        .map_err(|e| err(&e))?;
    let orientation = image::ImageDecoder::orientation(&mut decoder)
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder).map_err(|e| err(&e))?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// The prompt sent to the model for `instruction`. With reference photos the
/// instruction is expected to name them (`<image2>`, ...).
pub fn guidance_prompt(instruction: &str, with_references: bool) -> String {
    if with_references {
        format!(
            "The photo <image1> contains a red outline and red tint marking the region to edit. {} \
             Use the other photos only as a reference for the content to place there. \
             Apply the change only inside the marked region of <image1>. \
             Remove the red outline and tint, match the lighting, perspective and scale, \
             and keep everything outside the region exactly the same.",
            instruction.trim()
        )
    } else {
        format!(
            "The photo contains a red outline and red tint marking the region to edit. {} \
             Apply the change only inside the marked region. \
             Remove the red outline and tint, and keep everything outside the region exactly the same.",
            instruction.trim()
        )
    }
}

/// The engine request for a prepared edit: the marked source first, then the
/// references.
pub fn build_gen_request(req: &MaskedEditRequest, marked_path: &Path) -> GenRequest {
    let mut images = vec![marked_path.to_string_lossy().into_owned()];
    images.extend(
        req.references
            .iter()
            .map(|p| p.to_string_lossy().into_owned()),
    );
    GenRequest {
        mode: MODE.into(),
        prompt: guidance_prompt(&req.instruction, !req.references.is_empty()),
        images,
        preset: req.preset.clone(),
        aspect: "1:1".into(), // ignored: the canvas comes from the first image
        seed: req.seed,
        transparent: false,
    }
}

/// Validated, decoded inputs of a request.
#[derive(Debug)]
pub struct Prepared {
    pub source: RgbaImage,
    pub mask: GrayImage,
}

/// Validates the request before anything is queued.
pub fn prepare(req: &MaskedEditRequest) -> Result<Prepared, String> {
    let instruction = req.instruction.trim();
    if instruction.is_empty() {
        return Err("Describe the change to make in the selected area.".into());
    }
    if instruction.chars().count() > MAX_INSTRUCTION_CHARS {
        return Err(format!(
            "The instruction is longer than {MAX_INSTRUCTION_CHARS} characters."
        ));
    }
    if req.feather_px > crate::masked_edit::MAX_FEATHER_PX {
        return Err(MaskedEditError::InvalidFeather(req.feather_px).to_string());
    }
    if req.references.len() > MAX_REFERENCES {
        return Err(format!(
            "At most {MAX_REFERENCES} reference photos are supported."
        ));
    }
    for reference in &req.references {
        if !reference.is_file() {
            return Err(format!(
                "Cannot read the reference {}.",
                reference.display()
            ));
        }
        if reference == &req.source_path {
            return Err(
                "The reference must be a different photo than the one being edited.".into(),
            );
        }
    }
    let source = open_upright(&req.source_path)?.to_rgba8();
    if u64::from(source.width()) * u64::from(source.height()) > MAX_PIXELS {
        return Err("The source image is too large for masked editing.".into());
    }
    let mask = image::load_from_memory_with_format(&req.mask_png, image::ImageFormat::Png)
        .map_err(|e| format!("The mask is not a valid PNG: {e}"))?
        .to_luma8();
    validate_mask(&source, &mask).map_err(|e| e.to_string())?;
    Ok(Prepared { source, mask })
}

/// Model input: the source flattened onto white (the model has no alpha),
/// tinted red and outlined inside the mask. The outline is the mask minus its
/// square erosion, `max(4, width/300)` pixels wide.
pub fn mark_region(source: &RgbaImage, mask: &GrayImage) -> RgbImage {
    let (w, h) = source.dimensions();
    let radius = (w / 300).max(4) as i64;
    let inside = |x: i64, y: i64| mask.get_pixel(x as u32, y as u32)[0] >= 128;
    // Separable erosion by prefix sums of "outside" counts. Windows are
    // clipped at the border (equivalent to border replication), so a mask
    // touching the image edge gets no outline there.
    let (wi, hi) = (w as i64, h as i64);
    let window = |i: i64, n: i64| ((i - radius).max(0), (i + radius).min(n - 1));
    let mut horizontal = vec![false; (w * h) as usize];
    for y in 0..hi {
        let mut prefix = vec![0i64; w as usize + 1];
        for x in 0..wi {
            prefix[x as usize + 1] = prefix[x as usize] + i64::from(!inside(x, y));
        }
        for x in 0..wi {
            let (lo, hi_x) = window(x, wi);
            horizontal[(y * wi + x) as usize] =
                prefix[hi_x as usize + 1] - prefix[lo as usize] == 0;
        }
    }
    let mut eroded = vec![false; (w * h) as usize];
    for x in 0..wi {
        let mut prefix = vec![0i64; h as usize + 1];
        for y in 0..hi {
            prefix[y as usize + 1] =
                prefix[y as usize] + i64::from(!horizontal[(y * wi + x) as usize]);
        }
        for y in 0..hi {
            let (lo, hi_y) = window(y, hi);
            eroded[(y * wi + x) as usize] = prefix[hi_y as usize + 1] - prefix[lo as usize] == 0;
        }
    }

    let mut out = RgbImage::new(w, h);
    for (x, y, px) in source.enumerate_pixels() {
        let a = px[3] as f32 / 255.0;
        let flat = |c: u8| c as f32 * a + 255.0 * (1.0 - a);
        let base = [flat(px[0]), flat(px[1]), flat(px[2])];
        let i = (y * w + x) as usize;
        let selected = inside(x as i64, y as i64);
        let rgb = if selected && !eroded[i] {
            [255.0, 0.0, 0.0] // outline
        } else if selected {
            [
                base[0] * (1.0 - TINT) + 255.0 * TINT,
                base[1] * (1.0 - TINT),
                base[2] * (1.0 - TINT),
            ]
        } else {
            base
        };
        out.put_pixel(
            x,
            y,
            Rgb([
                rgb[0].round().clamp(0.0, 255.0) as u8,
                rgb[1].round().clamp(0.0, 255.0) as u8,
                rgb[2].round().clamp(0.0, 255.0) as u8,
            ]),
        );
    }
    out
}

/// Resamples the raw model output to the source canvas and composites it
/// through the mask. The engine returns its own canvas size, so resampling is
/// always needed; it is never cropped.
pub fn finish(
    source: &RgbaImage,
    mask: &GrayImage,
    raw: &image::DynamicImage,
    feather_px: u32,
) -> Result<RgbaImage, MaskedEditError> {
    let generated = if raw.dimensions() == source.dimensions() {
        raw.to_rgba8()
    } else {
        image::imageops::resize(
            &raw.to_rgba8(),
            source.width(),
            source.height(),
            FilterType::Lanczos3,
        )
    };
    composite(source, &generated, mask, &MaskedEditOptions { feather_px })
}

fn encode_png(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(buf.into_inner())
}

/// Runs one masked edit. `options.output_dir` is a folder (a timestamped name
/// is picked) or an exact `.png` path. Intermediate files live in an
/// app-owned temp folder that is removed before returning; only the final
/// composite is published, and the source is never written to.
pub async fn run_masked_edit(
    req: MaskedEditRequest,
    options: GenerateOptions<'_>,
) -> Result<GenResult, RunError> {
    let started = Instant::now();
    let cfg = options.config;
    let out = options.output_dir.clone();

    let request = req.clone();
    let Prepared { source, mask } = tokio::task::spawn_blocking(move || prepare(&request))
        .await
        .map_err(|e| RunError::Request(e.to_string()))??;

    let temp_root = cfg.runtime_dir.join("masked-tmp");
    tokio::fs::create_dir_all(&temp_root)
        .await
        .map_err(|e| RunError::Request(format!("Cannot create the temporary folder: {e}")))?;
    let temp = tempfile::Builder::new()
        .prefix("qil-masked-")
        .tempdir_in(&temp_root)
        .map_err(|e| RunError::Request(format!("Cannot create the temporary folder: {e}")))?;

    let result = run_in_temp(&req, source, mask, options, temp.path(), &out, started).await;

    // The engine only ever reads the uploaded copy and the raw result is
    // downloaded by us, so nothing outside this process writes into `temp`.
    match (temp.close(), result) {
        (Ok(()), result) => result,
        (Err(cleanup), Ok(done)) => {
            eprintln!("masked edit: could not remove temporary folder: {cleanup}");
            Ok(done)
        }
        (Err(cleanup), Err(RunError::Request(message))) => Err(RunError::Request(format!(
            "{message}; could not remove temporary files: {cleanup}"
        ))),
        (Err(cleanup), Err(other)) => {
            eprintln!("masked edit: could not remove temporary folder: {cleanup}");
            Err(other)
        }
    }
}

async fn run_in_temp(
    req: &MaskedEditRequest,
    source: RgbaImage,
    mask: GrayImage,
    options: GenerateOptions<'_>,
    temp: &Path,
    out: &Path,
    started: Instant,
) -> Result<GenResult, RunError> {
    let cfg = options.config;
    let marked_path = temp.join("marked.png");
    let marked_png = {
        let (s, m) = (source.clone(), mask.clone());
        tokio::task::spawn_blocking(move || {
            let marked = mark_region(&s, &m);
            let mut buf = std::io::Cursor::new(Vec::new());
            marked
                .write_to(&mut buf, image::ImageFormat::Png)
                .map(|()| buf.into_inner())
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| RunError::Request(e.to_string()))??
    };
    tokio::fs::write(&marked_path, marked_png)
        .await
        .map_err(|e| RunError::Request(format!("Cannot write the temporary image: {e}")))?;

    let raw_path = temp.join("raw.png");
    let gen_req = build_gen_request(req, &marked_path);
    let raw = run_with_control(
        gen_req,
        GenerateOptions {
            config: cfg,
            output_dir: raw_path.clone(),
            control: options.control,
            on_progress: options.on_progress,
        },
    )
    .await?;

    let raw_bytes = tokio::fs::read(&raw.path)
        .await
        .map_err(|e| RunError::Request(format!("Cannot read the engine result: {e}")))?;
    let (feather, src, msk) = (req.feather_px, source, mask);
    let final_png = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, u32, u32), String> {
        let raw_img = image::load_from_memory(&raw_bytes).map_err(|e| e.to_string())?;
        let done = finish(&src, &msk, &raw_img, feather).map_err(|e| e.to_string())?;
        Ok((encode_png(&done)?, done.width(), done.height()))
    })
    .await
    .map_err(|e| RunError::Request(e.to_string()))??;
    let (png_bytes, width, height) = final_png;

    let p = preset(&req.preset);
    let instruction = req.instruction.trim().to_string();
    let extra = format!(
        "Mode: {MODE}\nGuidance: {STRATEGY_ID}\nFeather: {}px\nReferences: {}",
        req.feather_px,
        req.references.len()
    );
    let png = embed_generation_metadata(
        &png_bytes,
        &GenerationMetadata {
            prompt: &instruction,
            seed: raw.seed,
            steps: raw.steps,
            width,
            height,
            preset: &req.preset,
            extra: Some(&extra),
        },
    )?;

    let proposed = if out
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("png"))
    {
        out.to_path_buf()
    } else {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        out.join(format!("{stamp}_{MODE}_{}.png", raw.seed))
    };
    let file = unique_output_path(&proposed)?;
    if let Some(dir) = file.parent() {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| RunError::Request(e.to_string()))?;
    }
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image.png".into());
    let part = file.with_file_name(format!(".{name}.{:08x}.part", rand::random::<u32>()));
    if let Err(e) = tokio::fs::write(&part, png).await {
        return Err(cleanup_part(&part, e.to_string()).await);
    }
    if let Err(e) = tokio::fs::rename(&part, &file).await {
        return Err(cleanup_part(&part, format!("Could not publish the image: {e}")).await);
    }

    let result = GenResult {
        path: file.to_string_lossy().into_owned(),
        seed: raw.seed,
        width,
        height,
        seconds: started.elapsed().as_secs_f64(),
        sample_seconds: raw.sample_seconds,
        steps: p.steps,
        prompt: instruction.clone(),
        mode: MODE.into(),
        preset: req.preset.clone(),
    };
    if should_save_json(cfg.save_json) {
        let round1 = |x: f64| (x * 10.0).round() / 10.0;
        let meta = json!({
            "prompt": instruction, "mode": MODE, "preset": req.preset,
            "guidance": STRATEGY_ID, "featherPx": req.feather_px,
            "seed": raw.seed, "steps": raw.steps, "width": width, "height": height,
            "references": std::iter::once(&req.source_path)
                .chain(req.references.iter())
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            "seconds": round1(result.seconds), "sampleSeconds": round1(raw.sample_seconds),
            "model": "Qwen-Image-2.1", "dit": DIT.name, "encoder": cfg.profile.encoder().name,
            "app": concat!("PicaLum QwenImage ", env!("CARGO_PKG_VERSION")),
            "created": chrono::Local::now().to_rfc3339(),
        });
        let _ = tokio::fs::write(
            file.with_extension("json"),
            serde_json::to_vec_pretty(&meta).unwrap(),
        )
        .await;
    }
    Ok(result)
}

async fn cleanup_part(part: &Path, message: String) -> RunError {
    match remove_temp_file(part).await {
        Ok(()) => RunError::Request(message),
        Err(e) => RunError::Request(format!("{message}; could not remove temporary image: {e}")),
    }
}

/// Text for a failed run, mirroring [`crate::generate::run`].
pub fn error_text(error: RunError, cfg: &crate::Config) -> String {
    run_error_text(error, cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgba};

    fn gradient(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, y| {
            Rgba([(x % 256) as u8, (y % 256) as u8, 90, 255])
        })
    }

    fn rect_mask(w: u32, h: u32, x0: u32, y0: u32, x1: u32, y1: u32) -> GrayImage {
        GrayImage::from_fn(w, h, |x, y| {
            Luma([if x >= x0 && x < x1 && y >= y0 && y < y1 {
                255
            } else {
                0
            }])
        })
    }

    fn mask_png(mask: &GrayImage) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        mask.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    fn request(dir: &Path, src: &RgbaImage, mask: &GrayImage) -> MaskedEditRequest {
        let path = dir.join("src.png");
        src.save(&path).unwrap();
        MaskedEditRequest {
            source_path: path,
            mask_png: mask_png(mask),
            instruction: "Make it red.".into(),
            preset: "draft".into(),
            seed: Some(1),
            feather_px: 4,
            references: vec![],
        }
    }

    #[test]
    fn prompt_embeds_the_instruction_between_fixed_guidance() {
        let p = guidance_prompt("  Make the saddle brown. ", false);
        assert!(p.contains("region to edit. Make the saddle brown. Apply the change only"));
        assert!(!p.contains("<image"));
    }

    #[test]
    fn reference_prompt_names_the_marked_photo_and_keeps_the_instruction() {
        let p = guidance_prompt("Place the dog from <image2> here.", true);
        assert!(p.starts_with("The photo <image1> contains"));
        assert!(p.contains("Place the dog from <image2> here. Use the other photos only"));
        assert!(p.contains("marked region of <image1>"));
    }

    #[test]
    fn engine_request_puts_the_marked_source_first_then_the_references() {
        let dir = tempfile::tempdir().unwrap();
        let src = gradient(8, 8);
        let mask = rect_mask(8, 8, 1, 1, 4, 4);
        let mut req = request(dir.path(), &src, &mask);
        let marked = dir.path().join("marked.png");
        let plain = build_gen_request(&req, &marked);
        assert_eq!(plain.images, vec![marked.to_string_lossy().to_string()]);
        assert_eq!(plain.mode, MODE);

        req.references = vec![dir.path().join("a.png"), dir.path().join("b.png")];
        let with_refs = build_gen_request(&req, &marked);
        assert_eq!(with_refs.images.len(), 3);
        assert_eq!(with_refs.images[0], marked.to_string_lossy());
        assert!(with_refs.images[1].ends_with("a.png") && with_refs.images[2].ends_with("b.png"));
        assert!(with_refs.prompt.contains("<image1>"));
    }

    #[test]
    fn prepare_validates_references() {
        let dir = tempfile::tempdir().unwrap();
        let src = gradient(8, 8);
        let mask = rect_mask(8, 8, 1, 1, 4, 4);
        let ok_ref = dir.path().join("ref.png");
        gradient(4, 4).save(&ok_ref).unwrap();

        let mut req = request(dir.path(), &src, &mask);
        req.references = vec![ok_ref.clone()];
        assert!(prepare(&req).is_ok());

        req.references = vec![dir.path().join("missing.png")];
        assert!(prepare(&req)
            .unwrap_err()
            .contains("Cannot read the reference"));

        req.references = vec![req.source_path.clone()];
        assert!(prepare(&req).unwrap_err().contains("different photo"));

        req.references = vec![ok_ref; MAX_REFERENCES + 1];
        assert!(prepare(&req).unwrap_err().contains("At most"));
    }

    #[test]
    fn prepare_accepts_a_valid_request_and_leaves_the_source_alone() {
        let dir = tempfile::tempdir().unwrap();
        let src = gradient(40, 30);
        let mask = rect_mask(40, 30, 10, 10, 20, 20);
        let req = request(dir.path(), &src, &mask);
        let before = std::fs::read(&req.source_path).unwrap();
        let prepared = prepare(&req).unwrap();
        assert_eq!(prepared.source.dimensions(), (40, 30));
        assert_eq!(prepared.mask, mask);
        assert_eq!(std::fs::read(&req.source_path).unwrap(), before);
    }

    #[test]
    fn prepare_rejects_bad_requests() {
        let dir = tempfile::tempdir().unwrap();
        let src = gradient(40, 30);
        let good = rect_mask(40, 30, 10, 10, 20, 20);

        let mut req = request(dir.path(), &src, &good);
        req.instruction = "   ".into();
        assert!(prepare(&req).unwrap_err().contains("Describe"));

        let mut req = request(dir.path(), &src, &good);
        req.feather_px = 10_000;
        assert!(prepare(&req).unwrap_err().contains("feather"));

        let empty = GrayImage::new(40, 30);
        let req = request(dir.path(), &src, &empty);
        assert!(prepare(&req).unwrap_err().contains("no editable area"));

        let wrong = rect_mask(41, 30, 1, 1, 5, 5);
        let req = request(dir.path(), &src, &wrong);
        assert!(prepare(&req).unwrap_err().contains("mask is 41x30"));

        let mut req = request(dir.path(), &src, &good);
        req.mask_png = b"not a png".to_vec();
        assert!(prepare(&req).unwrap_err().contains("not a valid PNG"));
    }

    #[test]
    fn marked_image_tints_and_outlines_only_the_mask() {
        let src = RgbaImage::from_pixel(100, 100, Rgba([100, 100, 100, 255]));
        let mask = rect_mask(100, 100, 30, 30, 70, 70);
        let marked = mark_region(&src, &mask);
        assert_eq!(*marked.get_pixel(5, 5), Rgb([100, 100, 100]));
        assert_eq!(*marked.get_pixel(30, 50), Rgb([255, 0, 0]), "outline");
        let inner = marked.get_pixel(50, 50);
        assert!(
            inner[0] > 100 && inner[1] < 100 && inner[2] < 100,
            "tint {inner:?}"
        );
    }

    #[test]
    fn marked_outline_is_wide_but_not_filled() {
        let src = RgbaImage::from_pixel(60, 60, Rgba([10, 10, 10, 255]));
        let mask = GrayImage::from_pixel(60, 60, Luma([255]));
        let marked = mark_region(&src, &mask);
        // Border replication: a full mask has no inner edge, so no outline.
        assert!(marked.pixels().all(|p| p[0] != 255 || p[1] != 0));
    }

    #[test]
    fn finish_resamples_the_raw_result_and_keeps_pixels_outside_the_mask() {
        let src = gradient(150, 100);
        let mask = rect_mask(150, 100, 40, 30, 90, 70);
        let raw = image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            120,
            80,
            Rgba([250, 10, 10, 255]),
        ));
        let out = finish(&src, &mask, &raw, 3).unwrap();
        assert_eq!(out.dimensions(), (150, 100));
        for (x, y, p) in out.enumerate_pixels() {
            if mask.get_pixel(x, y)[0] == 0 {
                assert_eq!(p, src.get_pixel(x, y));
            }
        }
        assert_eq!(*out.get_pixel(65, 50), Rgba([250, 10, 10, 255]));
    }
}
