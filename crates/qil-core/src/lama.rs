//! LaMa object erasing: verified weights and a small, testable installer.
//!
//! The model is a TorchScript file loaded by the app's Python environment. This
//! module only owns the artifact lifecycle; execution lives in the app layer.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use serde::Serialize;

use crate::config::Config;
use crate::download::{self, Progress};
use crate::generate::{should_save_json, unique_output_path};
use crate::run_control::{RunControl, RunError};

const WORKER: &str = include_str!("lama_worker.py");

pub const MODEL_NAME: &str = "big-lama.pt";
pub const MODEL_URL: &str =
    "https://github.com/Sanster/models/releases/download/add_big_lama/big-lama.pt";
pub const MODEL_SIZE: u64 = 205_669_692;
pub const MODEL_SHA256: &str = "344c77bbcb158f17dd143070d1e789f38a66c04202311ae3a258ef66667a9ea9";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LamaState {
    Missing,
    Ready,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LamaStatus {
    pub state: LamaState,
    pub path: String,
    pub size: u64,
    pub expected_size: u64,
}

impl LamaStatus {
    pub fn is_ready(&self) -> bool {
        self.state == LamaState::Ready
    }
}

pub fn model_path(cfg: &Config) -> PathBuf {
    cfg.models_dir.join("lama").join(MODEL_NAME)
}

fn part_path(dest: &Path) -> PathBuf {
    dest.with_extension("pt.part")
}

/// Reports whether the exact pinned artifact is present and verified.
pub fn status(cfg: &Config) -> Result<LamaStatus, String> {
    status_at(&model_path(cfg), MODEL_SIZE, MODEL_SHA256)
}

pub fn status_at(
    path: &Path,
    expected_size: u64,
    expected_sha: &str,
) -> Result<LamaStatus, String> {
    let size = match std::fs::metadata(path) {
        Ok(meta) => meta.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LamaStatus {
                state: LamaState::Missing,
                path: path.to_string_lossy().into_owned(),
                size: 0,
                expected_size,
            });
        }
        Err(error) => return Err(error.to_string()),
    };
    let state = if size == expected_size
        && download::sha256(path, &mut |_| {})?.eq_ignore_ascii_case(expected_sha)
    {
        LamaState::Ready
    } else {
        LamaState::Invalid
    };
    Ok(LamaStatus {
        state,
        path: path.to_string_lossy().into_owned(),
        size,
        expected_size,
    })
}

/// One download at a time. The guard releases the flag even after an error.
pub struct InstallGuard;

static INSTALLING: AtomicBool = AtomicBool::new(false);

impl InstallGuard {
    pub fn acquire() -> Option<Self> {
        INSTALLING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| Self)
    }
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        INSTALLING.store(false, Ordering::SeqCst);
    }
}

/// Downloads, verifies and atomically publishes the pinned model.
pub fn install(cfg: &Config, on_progress: &mut dyn FnMut(Progress)) -> Result<(), String> {
    let _guard = InstallGuard::acquire()
        .ok_or_else(|| "A LaMa model download is already running.".to_string())?;
    install_with(
        &model_path(cfg),
        MODEL_URL,
        MODEL_SIZE,
        MODEL_SHA256,
        |url, dest, size, progress| download::fetch(url, dest, size, progress),
        on_progress,
    )
}

/// Installer seam: tests provide `fetch` and never touch the network.
pub fn install_with<F>(
    dest: &Path,
    url: &str,
    expected_size: u64,
    expected_sha: &str,
    fetch: F,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<(), String>
where
    F: FnOnce(&str, &Path, Option<u64>, &mut dyn FnMut(Progress)) -> Result<(), String>,
{
    if status_at(dest, expected_size, expected_sha)?.is_ready() {
        return Ok(());
    }
    if dest.exists() {
        std::fs::remove_file(dest).map_err(|e| e.to_string())?;
    }
    fetch(url, dest, Some(expected_size), on_progress)?;
    let sum = download::sha256(dest, on_progress)?;
    if !sum.eq_ignore_ascii_case(expected_sha) {
        let _ = std::fs::remove_file(dest);
        let _ = std::fs::remove_file(part_path(dest));
        return Err("LaMa model failed its checksum and was deleted; try again.".into());
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct EraseRequest {
    pub source_path: PathBuf,
    pub mask_png: Vec<u8>,
}

pub struct EraseOptions<'a> {
    pub config: &'a Config,
    pub control: RunControl,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EraseResult {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub seconds: f64,
}

/// True when the mask PNG contains at least one painted pixel.
pub fn mask_has_pixels(png: &[u8]) -> Result<bool, String> {
    let image = image::load_from_memory(png)
        .map_err(|error| format!("Cannot decode mask PNG: {error}"))?
        .to_luma8();
    if u64::from(image.width()) * u64::from(image.height()) > crate::masked_run::MAX_PIXELS {
        return Err("The mask dimensions exceed the supported limit.".into());
    }
    Ok(image.pixels().any(|pixel| pixel[0] > 0))
}

fn ensure_worker(cfg: &Config) -> Result<PathBuf, String> {
    let dir = cfg.runtime_dir.join("lama");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join("worker.py");
    let current = std::fs::read_to_string(&path).ok();
    if current.as_deref() != Some(WORKER) {
        std::fs::write(&path, WORKER).map_err(|error| error.to_string())?;
    }
    Ok(path)
}

fn worker_tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    lines
        .iter()
        .rev()
        .take(4)
        .rev()
        .copied()
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Runs LaMa in the app's Python environment and publishes an atomic PNG.
pub async fn erase(req: EraseRequest, options: EraseOptions<'_>) -> Result<EraseResult, RunError> {
    let cfg = options.config;
    if !status(cfg)?.is_ready() {
        return Err(RunError::Request(
            "The LaMa model is not installed. Download it first.".into(),
        ));
    }
    let mask = req.mask_png.clone();
    let painted = tokio::task::spawn_blocking(move || mask_has_pixels(&mask))
        .await
        .map_err(|error| RunError::Request(error.to_string()))??;
    if !painted {
        return Err(RunError::Request("Paint the area to erase first.".into()));
    }

    let temp_root = cfg.runtime_dir.join("lama-tmp");
    std::fs::create_dir_all(&temp_root).map_err(|error| RunError::Request(error.to_string()))?;
    let dir = tempfile::Builder::new()
        .prefix("erase-")
        .tempdir_in(&temp_root)
        .map_err(|error| RunError::Request(error.to_string()))?;
    let mask_path = dir.path().join("mask.png");
    tokio::fs::write(&mask_path, &req.mask_png)
        .await
        .map_err(|error| RunError::Request(error.to_string()))?;
    let output_path = dir.path().join("output.png");
    let worker = ensure_worker(cfg).map_err(RunError::Request)?;
    let model = model_path(cfg);
    let source = req.source_path;

    let mut command = tokio::process::Command::new(cfg.python());
    command
        .arg(&worker)
        .arg(&model)
        .arg(&source)
        .arg(&mask_path)
        .arg(&output_path)
        .env("PYTHONUNBUFFERED", "1")
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(uuid) = cfg
        .gpu_selection
        .uuid
        .as_deref()
        .filter(|uuid| !uuid.trim().is_empty())
    {
        command.env("CUDA_VISIBLE_DEVICES", uuid);
    }

    let started = Instant::now();
    let output = {
        let mut control = options.control;
        control
            .race(async { command.output().await.map_err(|error| error.to_string()) })
            .await?
    };
    if !output.status.success() {
        return Err(RunError::Request(format!(
            "LaMa could not erase the area: {}",
            worker_tail(&output.stderr)
        )));
    }
    let (width, height) = image::image_dimensions(&output_path)
        .map_err(|error| RunError::Request(format!("Cannot read the erased image: {error}")))?;
    let seconds = started.elapsed().as_secs_f64();
    let published = publish_output(
        &output_path,
        &cfg.output_dir,
        &source,
        width,
        height,
        seconds,
        cfg.save_json,
    )
    .map_err(RunError::Request)?;
    Ok(EraseResult {
        path: published.to_string_lossy().into_owned(),
        width,
        height,
        seconds,
    })
}

fn publish_output(
    generated: &Path,
    output_dir: &Path,
    source: &Path,
    width: u32,
    height: u32,
    seconds: f64,
    save_json: bool,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(output_dir).map_err(|error| error.to_string())?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let proposed = output_dir.join(format!("{stamp}_erase.png"));
    let dest = unique_output_path(&proposed)?;
    let name = dest
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "erase.png".into());
    let part = dest.with_file_name(format!(".{name}.{:08x}.part", rand::random::<u32>()));
    if let Err(error) = std::fs::copy(generated, &part) {
        let _ = std::fs::remove_file(&part);
        return Err(error.to_string());
    }
    if let Err(error) = std::fs::rename(&part, &dest) {
        let _ = std::fs::remove_file(&part);
        return Err(format!("Could not publish the erased image: {error}"));
    }
    if should_save_json(save_json) {
        let round1 = |x: f64| (x * 10.0).round() / 10.0;
        let meta = serde_json::json!({
            "prompt": "Erase",
            "mode": "erase",
            "model": "LaMa",
            "width": width,
            "height": height,
            "seconds": round1(seconds),
            "references": [source.to_string_lossy()],
            "app": concat!("PicaLum QwenImage ", env!("CARGO_PKG_VERSION")),
            "created": chrono::Local::now().to_rfc3339(),
        });
        let _ = std::fs::write(
            dest.with_extension("json"),
            serde_json::to_vec_pretty(&meta).unwrap(),
        );
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sha(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(bytes))
    }

    fn fixture(dir: &tempfile::TempDir, bytes: &[u8]) -> PathBuf {
        let path = dir.path().join(MODEL_NAME);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(bytes).unwrap();
        path
    }

    #[test]
    fn status_distinguishes_missing_ready_and_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MODEL_NAME);
        let expected = sha(b"lama");
        assert_eq!(
            status_at(&path, 4, &expected).unwrap().state,
            LamaState::Missing
        );

        std::fs::write(&path, b"lama").unwrap();
        assert_eq!(
            status_at(&path, 4, &expected).unwrap().state,
            LamaState::Ready
        );

        std::fs::write(&path, b"nope").unwrap();
        assert_eq!(
            status_at(&path, 4, &expected).unwrap().state,
            LamaState::Invalid
        );
    }

    #[test]
    fn wrong_checksum_is_deleted_and_can_be_retried() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MODEL_NAME);
        let expected = sha(b"good");

        let error = install_with(
            &path,
            "test://lama",
            4,
            &expected,
            |_, dest, _, _| std::fs::write(dest, b"bad!").map_err(|e| e.to_string()),
            &mut |_| {},
        )
        .unwrap_err();
        assert!(error.contains("checksum"));
        assert!(!path.exists());
        assert!(!part_path(&path).exists());

        let mut calls = 0;
        install_with(
            &path,
            "test://lama",
            4,
            &expected,
            |_, dest, _, _| {
                calls += 1;
                std::fs::write(dest, b"good").map_err(|e| e.to_string())
            },
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(std::fs::read(&path).unwrap(), b"good");
        assert_eq!(
            status_at(&path, 4, &expected).unwrap().state,
            LamaState::Ready
        );
    }

    #[test]
    fn a_ready_model_is_not_downloaded_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture(&dir, b"good");
        let expected = sha(b"good");
        let mut fetched = false;
        install_with(
            &path,
            "test://lama",
            4,
            &expected,
            |_, _, _, _| {
                fetched = true;
                Ok(())
            },
            &mut |_| {},
        )
        .unwrap();
        assert!(!fetched);
    }

    #[test]
    fn only_one_install_can_run_at_a_time() {
        let first = InstallGuard::acquire().expect("first install");
        assert!(InstallGuard::acquire().is_none());
        drop(first);
        assert!(InstallGuard::acquire().is_some());
    }

    #[test]
    fn empty_masks_are_rejected_and_painted_masks_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mask.png");
        image::GrayImage::from_pixel(2, 2, image::Luma([0u8]))
            .save(&path)
            .unwrap();
        assert!(!mask_has_pixels(&std::fs::read(&path).unwrap()).unwrap());

        image::GrayImage::from_pixel(2, 2, image::Luma([255u8]))
            .save(&path)
            .unwrap();
        assert!(mask_has_pixels(&std::fs::read(&path).unwrap()).unwrap());
    }

    #[tokio::test]
    #[ignore = "requires LAMA_TEST_MODEL and runs the real TorchScript model"]
    async fn real_cpu_erase_publishes_a_source_sized_composite() {
        let model = std::env::var("LAMA_TEST_MODEL").expect("LAMA_TEST_MODEL");
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config {
            runtime_dir: crate::config::default_runtime_dir(),
            models_dir: dir.path().join("models"),
            output_dir: dir.path().join("out"),
            ..Config::default()
        };
        std::fs::create_dir_all(cfg.models_dir.join("lama")).unwrap();
        std::fs::copy(model, model_path(&cfg)).unwrap();

        let source = dir.path().join("source.png");
        let mut image = image::RgbImage::from_pixel(64, 64, image::Rgb([40, 120, 60]));
        for x in 20..44 {
            for y in 20..44 {
                image.put_pixel(x, y, image::Rgb([200, 20, 20]));
            }
        }
        image.save(&source).unwrap();
        let mut mask = image::GrayImage::from_pixel(64, 64, image::Luma([0u8]));
        for x in 20..44 {
            for y in 20..44 {
                mask.put_pixel(x, y, image::Luma([255u8]));
            }
        }
        let mask_path = dir.path().join("mask.png");
        mask.save(&mask_path).unwrap();

        let (_cancel, cancel_rx) = tokio::sync::watch::channel(false);
        let result = erase(
            EraseRequest {
                source_path: source,
                mask_png: std::fs::read(&mask_path).unwrap(),
            },
            EraseOptions {
                config: &cfg,
                control: RunControl::new(
                    cancel_rx,
                    tokio::time::Instant::now() + std::time::Duration::from_secs(300),
                ),
            },
        )
        .await
        .unwrap();

        assert_eq!((result.width, result.height), (64, 64));
        let output = image::open(&result.path).unwrap().to_rgb8();
        assert_eq!(output.get_pixel(0, 0), image.get_pixel(0, 0));
        assert_ne!(output.get_pixel(32, 32), image.get_pixel(32, 32));
        assert!(result.path.ends_with("_erase.png"));
    }

    #[test]
    fn publishing_never_overwrites_and_writes_erase_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let source = dir.path().join("source.png");
        let png = image::RgbImage::from_pixel(3, 2, image::Rgb([10, 20, 30]));
        png.save(&source).unwrap();
        let generated = dir.path().join("generated.png");
        png.save(&generated).unwrap();

        let first = publish_output(&generated, &out, &source, 3, 2, 1.25, true).unwrap();
        let second = publish_output(&generated, &out, &source, 3, 2, 1.25, true).unwrap();
        assert_ne!(first, second);
        assert!(first.exists() && second.exists());
        assert!(first.to_string_lossy().ends_with("_erase.png"));

        let meta: serde_json::Value =
            serde_json::from_slice(&std::fs::read(first.with_extension("json")).unwrap()).unwrap();
        assert_eq!(meta["mode"], "erase");
        assert_eq!(meta["model"], "LaMa");
        assert_eq!(meta["width"], 3);
        assert_eq!(meta["height"], 2);
        assert_eq!(meta["references"][0], source.to_string_lossy().as_ref());
        assert!(meta.get("seed").is_none());
    }
}
