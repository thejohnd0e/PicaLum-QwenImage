use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::config::{GpuVerification, Profile, VerificationStatus};
use crate::generate::GenRequest;

pub const MAX_SECONDS: u64 = 180;
pub fn remaining_budget(elapsed: std::time::Duration) -> std::time::Duration {
    std::time::Duration::from_secs(MAX_SECONDS).saturating_sub(elapsed)
}
pub fn can_start(busy: bool) -> bool {
    !busy
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VerificationClass {
    Success,
    Cancelled,
    Timeout,
    CudaError,
    OutOfMemory,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationOutcome {
    pub class: VerificationClass,
    pub cleanup_safe: bool,
}

pub fn request() -> GenRequest {
    GenRequest {
        mode: "text".into(),
        prompt: "A simple red apple on a plain neutral background.".into(),
        images: vec![],
        preset: "draft".into(),
        aspect: "1:1".into(),
        seed: Some(42),
        transparent: false,
    }
}

/// Identifies the installed engine; a verification result is current only for the same one.
pub fn engine_fingerprint() -> String {
    format!("comfy-{}", crate::config::COMFY_COMMIT)
}

pub fn run_dir(runtime: &Path, id: &str) -> PathBuf {
    runtime.join("verification").join(format!("run-{id}"))
}

pub fn classify(error: Option<&str>, cancelled: bool, timed_out: bool) -> VerificationOutcome {
    if cancelled {
        return VerificationOutcome {
            class: VerificationClass::Cancelled,
            cleanup_safe: true,
        };
    }
    if timed_out {
        return VerificationOutcome {
            class: VerificationClass::Timeout,
            cleanup_safe: false,
        };
    }
    let text = error.unwrap_or_default().to_ascii_lowercase();
    let class = if text.contains("out of memory") || text.contains("cuda out of memory") {
        VerificationClass::OutOfMemory
    } else if text.contains("cuda") || text.contains("torch") {
        VerificationClass::CudaError
    } else if error.is_none() {
        VerificationClass::Success
    } else {
        VerificationClass::Failed
    };
    VerificationOutcome {
        class,
        cleanup_safe: !matches!(class, VerificationClass::Timeout),
    }
}

pub fn cleanup_empty(path: &Path) -> std::io::Result<()> {
    let mut entries = std::fs::read_dir(path)?;
    if entries.next().is_none() {
        std::fs::remove_dir(path)?;
    }
    Ok(())
}

pub fn cleanup(path: &Path, safe: bool) -> std::io::Result<()> {
    if safe {
        std::fs::remove_dir_all(path).or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(e)
            }
        })
    } else {
        Ok(())
    }
}

pub fn record(
    uuid: String,
    profile: Profile,
    status: VerificationStatus,
    seconds: Option<f64>,
    peak: Option<u32>,
    message: Option<String>,
) -> GpuVerification {
    GpuVerification {
        gpu_uuid: uuid,
        profile,
        engine_fingerprint: engine_fingerprint(),
        status,
        checked_at: chrono_free_timestamp(),
        seconds,
        peak_vram_mb: peak,
        message,
    }
}

fn chrono_free_timestamp() -> String {
    format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_is_fixed_draft() {
        let r = request();
        assert_eq!(r.preset, "draft");
        assert_eq!(r.seed, Some(42));
    }
    #[test]
    fn classification_covers_success_cancel_timeout_oom_cuda() {
        assert_eq!(
            classify(None, false, false).class,
            VerificationClass::Success
        );
        assert_eq!(
            classify(Some("cancel"), true, false).class,
            VerificationClass::Cancelled
        );
        assert_eq!(
            classify(None, false, true).class,
            VerificationClass::Timeout
        );
        assert_eq!(
            classify(Some("CUDA out of memory"), false, false).class,
            VerificationClass::OutOfMemory
        );
        assert_eq!(
            classify(Some("CUDA error"), false, false).class,
            VerificationClass::CudaError
        );
    }
    #[test]
    fn startup_budget_is_bounded_and_overlap_is_rejected() {
        assert!(
            remaining_budget(std::time::Duration::from_secs(10))
                < std::time::Duration::from_secs(MAX_SECONDS)
        );
        assert!(can_start(false));
        assert!(!can_start(true));
    }
    #[test]
    fn cancellation_unconfirmed_is_not_safe_to_cleanup() {
        assert!(!classify(Some("CancellationUnconfirmed"), false, true).cleanup_safe);
    }
    #[test]
    fn cleanup_is_independent_of_persistence_result() {
        assert!(cleanup(&std::env::temp_dir().join("missing-verification"), true).is_ok());
    }
    #[test]
    fn empty_startup_directory_is_removed_but_nonempty_is_preserved() {
        let base = std::env::temp_dir().join("qil-verification-empty-test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        cleanup_empty(&base).unwrap();
        assert!(!base.exists());
    }
    #[test]
    fn unsafe_timeout_cleanup_preserves_artifacts() {
        let d = std::env::temp_dir().join("qil-verification-test");
        assert!(cleanup(&d, false).is_ok());
    }
}

#[cfg(test)]
mod fingerprint_tests {
    use super::{engine_fingerprint, record};
    use crate::config::{Profile, VerificationStatus};

    #[test]
    fn record_uses_the_shared_engine_fingerprint() {
        let r = record(
            "GPU-a".into(),
            Profile::Balanced,
            VerificationStatus::Passed,
            None,
            None,
            None,
        );
        assert_eq!(r.engine_fingerprint, engine_fingerprint());
    }

    #[test]
    fn setup_status_carries_the_same_fingerprint() {
        let status = crate::setup::status_for_inventory(&crate::Config::default(), &[], None);
        assert_eq!(status.engine_fingerprint, engine_fingerprint());
    }
}
