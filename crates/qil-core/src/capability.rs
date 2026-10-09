//! Runtime GPU capability proof. Obtained only from the bundled PyTorch, bound to the
//! selected UUID, and never inferred from a marketing name.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::engine::{check_runtime_gpu_identity, GpuIdentityCheck};
use crate::gpu::no_window;
use crate::gpu_profile::GpuCapability;

const PROBE_TIMEOUT: Duration = Duration::from_secs(120);
const PROBE_PREFIX: &str = "PICALUM_PROBE ";
const PROBE_SCRIPT: &str = "import json,torch\n\
assert torch.cuda.is_available(),'CUDA not available'\n\
assert torch.cuda.device_count()==1,'expected exactly one visible CUDA device'\n\
p=torch.cuda.get_device_properties(0)\n\
print('PICALUM_PROBE '+json.dumps({'uuid':str(getattr(p,'uuid','')),'major':p.major,'minor':p.minor,'archList':torch.cuda.get_arch_list()}))";

/// Raw facts reported by the bundled PyTorch for the single visible device.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeProbe {
    pub uuid: String,
    pub major: u32,
    pub minor: u32,
    pub arch_list: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    /// The runtime reported no usable UUID.
    MissingUuid,
    /// The runtime device is not the selected device.
    UuidMismatch,
    /// The runtime reported no compiled architectures.
    MissingArchList,
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingUuid => write!(f, "PyTorch did not report a GPU UUID."),
            Self::UuidMismatch => {
                write!(f, "PyTorch bound a different GPU than the selected UUID.")
            }
            Self::MissingArchList => write!(f, "PyTorch reported no compiled CUDA architectures."),
        }
    }
}

impl std::error::Error for ProbeError {}

/// Compatible when a compiled `sm_XY` cubin has the device's major and a minor not above
/// the device's. PTX (`compute_*`) is deliberately not trusted: JIT is slow and unreviewed.
fn arch_supports(arch_list: &[String], major: u32, minor: u32) -> bool {
    arch_list.iter().any(|arch| {
        let Some(digits) = arch.trim().strip_prefix("sm_") else {
            return false;
        };
        // Suffixes such as `sm_90a` are arch-specific and only run on that exact device.
        let Ok(value) = digits.parse::<u32>() else {
            return false;
        };
        value / 10 == major && value % 10 <= minor
    })
}

/// Validates a probe against the selected UUID and derives the capability verdict.
pub fn capability_from_probe(
    selected_uuid: &str,
    probe: &RuntimeProbe,
) -> Result<GpuCapability, ProbeError> {
    match check_runtime_gpu_identity(selected_uuid, Some(&probe.uuid)) {
        GpuIdentityCheck::Match => {}
        GpuIdentityCheck::NotFound | GpuIdentityCheck::Unverified => {
            return Err(ProbeError::MissingUuid)
        }
        GpuIdentityCheck::Mismatch => return Err(ProbeError::UuidMismatch),
    }
    if probe.arch_list.is_empty() {
        return Err(ProbeError::MissingArchList);
    }
    Ok(GpuCapability {
        major: probe.major,
        minor: probe.minor,
        compatible: arch_supports(&probe.arch_list, probe.major, probe.minor),
    })
}

pub fn parse_probe_output(stdout: &str) -> Result<RuntimeProbe, String> {
    let line = stdout
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix(PROBE_PREFIX))
        .ok_or("PyTorch capability probe printed no result.")?;
    serde_json::from_str(line).map_err(|e| format!("Unreadable PyTorch capability probe: {e}"))
}

/// Result of a completed probe, as persisted next to the runtime.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum CapabilityState {
    /// Prerequisites are not installed or not yet probed; nothing is proven.
    Pending,
    Proven {
        major: u32,
        minor: u32,
    },
    Rejected {
        major: u32,
        minor: u32,
    },
}

impl CapabilityState {
    pub fn from_capability(capability: Option<&GpuCapability>) -> Self {
        match capability {
            None => Self::Pending,
            Some(c) if c.compatible => Self::Proven {
                major: c.major,
                minor: c.minor,
            },
            Some(c) => Self::Rejected {
                major: c.major,
                minor: c.minor,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityProof {
    pub uuid: String,
    /// Deps marker fingerprint; a reinstall of the runtime invalidates the proof.
    pub runtime: String,
    pub capability: GpuCapability,
}

fn proof_path(cfg: &Config) -> std::path::PathBuf {
    cfg.runtime_dir.join("gpu_capability.json")
}

/// The marker is rewritten on every dependency (re)install, so its contents plus its
/// modification time identify one installation of the runtime.
fn runtime_fingerprint(cfg: &Config) -> Option<String> {
    let marker = cfg.deps_marker();
    let text = std::fs::read_to_string(&marker).ok()?;
    let modified = std::fs::metadata(&marker).ok()?.modified().ok()?;
    let nanos = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(format!("{}@{nanos}", text.trim()))
}

pub fn clear_proof(cfg: &Config) {
    let _ = std::fs::remove_file(proof_path(cfg));
}

pub fn save_proof(cfg: &Config, uuid: &str, capability: GpuCapability) -> Result<(), String> {
    let proof = CapabilityProof {
        uuid: uuid.to_string(),
        runtime: runtime_fingerprint(cfg).ok_or("PyTorch runtime is not installed.")?,
        capability,
    };
    let json = serde_json::to_string_pretty(&proof).map_err(|e| e.to_string())?;
    let tmp = proof_path(cfg).with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, proof_path(cfg)).map_err(|e| e.to_string())
}

/// The stored proof for `uuid`, only while the runtime it was taken from is still installed.
pub fn load_proof(cfg: &Config, uuid: &str) -> Option<GpuCapability> {
    let runtime = runtime_fingerprint(cfg)?;
    let proof: CapabilityProof =
        serde_json::from_str(&std::fs::read_to_string(proof_path(cfg)).ok()?).ok()?;
    (proof.runtime == runtime && proof.uuid == uuid).then_some(proof.capability)
}

/// Runs the bundled Python with only the selected UUID visible. Blocking and time-bounded.
pub fn run_probe(cfg: &Config, selected_uuid: &str) -> Result<RuntimeProbe, String> {
    let mut cmd = Command::new(cfg.python());
    cmd.args(["-c", PROBE_SCRIPT])
        .env("CUDA_VISIBLE_DEVICES", selected_uuid)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = no_window(&mut cmd)
        .spawn()
        .map_err(|e| format!("Could not run the PyTorch capability probe: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("probe stdout unavailable")?;
    let mut stderr = child.stderr.take().ok_or("probe stderr unavailable")?;
    let out = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let err = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break status,
            None if started.elapsed() > PROBE_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("The PyTorch capability probe timed out.".into());
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    };
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    if !status.success() {
        let tail: Vec<&str> = stderr.lines().rev().take(3).collect();
        return Err(format!(
            "The PyTorch capability probe failed: {}",
            tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
        ));
    }
    parse_probe_output(&stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(uuid: &str, major: u32, minor: u32, archs: &[&str]) -> RuntimeProbe {
        RuntimeProbe {
            uuid: uuid.into(),
            major,
            minor,
            arch_list: archs.iter().map(|a| a.to_string()).collect(),
        }
    }

    #[test]
    fn matching_uuid_and_native_arch_is_proven() {
        let cap = capability_from_probe(
            "GPU-aaaa-01",
            &probe("GPU-AAAA-01", 8, 6, &["sm_75", "sm_80", "sm_90"]),
        )
        .unwrap();
        assert_eq!(
            cap,
            GpuCapability {
                major: 8,
                minor: 6,
                compatible: true
            }
        );
    }

    #[test]
    fn mismatched_uuid_is_rejected_before_capability() {
        let result = capability_from_probe("GPU-aaaa", &probe("GPU-bbbb", 8, 6, &["sm_80"]));
        assert_eq!(result, Err(ProbeError::UuidMismatch));
    }

    #[test]
    fn missing_or_malformed_uuid_is_rejected() {
        for uuid in ["", "not-a-uuid"] {
            let result = capability_from_probe("GPU-aaaa", &probe(uuid, 8, 6, &["sm_80"]));
            assert_eq!(result, Err(ProbeError::MissingUuid), "{uuid:?}");
        }
    }

    #[test]
    fn empty_arch_list_is_rejected() {
        let result = capability_from_probe("GPU-aaaa", &probe("GPU-aaaa", 8, 6, &[]));
        assert_eq!(result, Err(ProbeError::MissingArchList));
    }

    #[test]
    fn unsupported_architecture_is_incompatible_not_an_error() {
        let cap = capability_from_probe(
            "GPU-aaaa",
            &probe("GPU-aaaa", 6, 1, &["sm_75", "sm_80", "sm_120"]),
        )
        .unwrap();
        assert!(!cap.compatible);
        assert_eq!((cap.major, cap.minor), (6, 1));
    }

    #[test]
    fn newer_device_minor_runs_older_minor_cubin_but_not_the_reverse() {
        assert!(arch_supports(&["sm_80".into()], 8, 6));
        assert!(!arch_supports(&["sm_86".into()], 8, 0));
        assert!(!arch_supports(&["sm_80".into()], 9, 0));
    }

    #[test]
    fn ptx_and_arch_specific_entries_are_not_trusted() {
        assert!(!arch_supports(&["compute_90".into()], 9, 0));
        assert!(!arch_supports(&["sm_90a".into()], 9, 0));
    }

    #[test]
    fn parses_last_probe_line_ignoring_noise() {
        let out = "warning\nPICALUM_PROBE {\"uuid\":\"GPU-a\",\"major\":8,\"minor\":6,\"archList\":[\"sm_80\"]}\n";
        assert_eq!(
            parse_probe_output(out).unwrap(),
            probe("GPU-a", 8, 6, &["sm_80"])
        );
        assert!(parse_probe_output("nothing here").is_err());
        assert!(parse_probe_output("PICALUM_PROBE {bad").is_err());
    }

    #[test]
    fn proof_round_trips_only_for_same_uuid_and_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config {
            runtime_dir: dir.path().to_path_buf(),
            ..Config::default()
        };
        let cap = GpuCapability {
            major: 8,
            minor: 6,
            compatible: true,
        };
        assert!(save_proof(&cfg, "GPU-a", cap).is_err(), "no runtime yet");
        std::fs::write(cfg.deps_marker(), "commit-1").unwrap();
        save_proof(&cfg, "GPU-a", cap).unwrap();
        assert_eq!(load_proof(&cfg, "GPU-a"), Some(cap));
        assert_eq!(load_proof(&cfg, "GPU-b"), None);
        std::fs::write(cfg.deps_marker(), "commit-2").unwrap();
        assert_eq!(load_proof(&cfg, "GPU-a"), None, "reinstall invalidates");
        save_proof(&cfg, "GPU-a", cap).unwrap();
        assert_eq!(load_proof(&cfg, "GPU-a"), Some(cap));
        // Same marker text rewritten later (what a real reinstall does) must still invalidate.
        std::thread::sleep(std::time::Duration::from_millis(30));
        std::fs::write(cfg.deps_marker(), "commit-2").unwrap();
        assert_eq!(load_proof(&cfg, "GPU-a"), None, "rewrite invalidates");
        save_proof(&cfg, "GPU-a", cap).unwrap();
        clear_proof(&cfg);
        assert_eq!(load_proof(&cfg, "GPU-a"), None);
    }

    #[test]
    fn state_reflects_pending_proven_rejected() {
        assert_eq!(
            CapabilityState::from_capability(None),
            CapabilityState::Pending
        );
        let ok = GpuCapability {
            major: 8,
            minor: 6,
            compatible: true,
        };
        let bad = GpuCapability {
            major: 6,
            minor: 1,
            compatible: false,
        };
        assert_eq!(
            CapabilityState::from_capability(Some(&ok)),
            CapabilityState::Proven { major: 8, minor: 6 }
        );
        assert_eq!(
            CapabilityState::from_capability(Some(&bad)),
            CapabilityState::Rejected { major: 6, minor: 1 }
        );
    }
}
