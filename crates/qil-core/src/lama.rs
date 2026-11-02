//! LaMa object erasing: verified weights and a small, testable installer.
//!
//! The model is a TorchScript file loaded by the app's Python environment. This
//! module only owns the artifact lifecycle; execution lives in the app layer.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::config::Config;
use crate::download::{self, Progress};

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
}
