//! First-run installer. Every step is idempotent: re-running skips what is done,
//! and interrupted model downloads resume.
//!
//! Layout under `runtime_dir`:
//!   bin/uv.exe              uv (manages Python and pip)
//!   python/                 Python 3.12 installed by uv
//!   ComfyUI/                ComfyUI at a pinned commit, with its .venv
//!   models/                 weights (unless models_dir points elsewhere)
//!   extra_model_paths.yaml  tells ComfyUI where the weights are

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::capability::{self, CapabilityState};
use crate::config::{Config, Profile, ProfileMode, COMFY_COMMIT, TORCH_INDEX, UV_VERSION};
use crate::download::{self, human_bytes};
use crate::gpu::{self, no_window};
use crate::gpu_profile::{self, GpuCapability, GpuRecommendation, GpuSupportReason};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub id: String,
    pub label: String,
    pub done: bool,
    /// Bytes still to download for this step (0 if none / done).
    pub download_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub ready: bool,
    pub steps: Vec<Step>,
    pub gpu: Option<gpu::Gpu>,
    pub gpus: Vec<GpuSetupDevice>,
    pub selected_uuid: Option<String>,
    pub effective_profile: Option<Profile>,
    pub recommendation: Option<GpuRecommendation>,
    pub eligibility_error: Option<SetupError>,
    /// Pending until the bundled PyTorch has proven the selected GPU; model steps appear only after.
    pub capability: CapabilityState,
    /// Same value stored in a verification record, so the UI can tell if a result is current.
    pub engine_fingerprint: String,
    pub problems: Vec<String>,
    pub download_bytes: u64,
    pub free_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GpuSetupDevice {
    pub gpu: gpu::Gpu,
    pub recommendation: GpuRecommendation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSetup {
    pub gpu: gpu::Gpu,
    pub profile: Profile,
    pub recommendation: GpuRecommendation,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum SetupError {
    NoNvidiaGpu,
    SelectedGpuMissing {
        uuid: String,
    },
    UnsupportedGpu {
        uuid: String,
        reason: GpuSupportReason,
    },
    DiscoveryFailed {
        message: String,
    },
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoNvidiaGpu => write!(formatter, "No NVIDIA GPU detected."),
            Self::SelectedGpuMissing { uuid } => {
                write!(formatter, "Selected GPU {uuid} is not present.")
            }
            Self::UnsupportedGpu { uuid, reason } => match reason {
                GpuSupportReason::UnknownCapability => write!(formatter, "GPU {uuid} compute capability could not be verified before download."),
                GpuSupportReason::InsufficientVram {
                    minimum_mb,
                    actual_mb,
                } => write!(
                    formatter,
                    "GPU {uuid} has {actual_mb} MB VRAM; at least {minimum_mb} MB is required."
                ),
                GpuSupportReason::DriverTooOld {
                    minimum_major,
                    actual_major,
                } => write!(
                    formatter,
                    "GPU {uuid} uses driver major {actual_major}; {minimum_major} or newer is required."
                ),
                GpuSupportReason::IncompatibleCapability { major, minor } => write!(
                    formatter,
                    "GPU {uuid} has unsupported compute capability {major}.{minor}."
                ),
            },
            Self::DiscoveryFailed { message } => write!(formatter, "GPU detection failed: {message}"),
        }
    }
}

impl std::error::Error for SetupError {}

#[derive(Debug, Clone, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum Event {
    Step {
        id: String,
        state: &'static str,
        detail: String,
    }, // state: running | done | skipped | error
    Progress {
        id: String,
        done: u64,
        total: u64,
        bytes_per_sec: f64,
    },
    Log {
        line: String,
    },
}

fn uv_ready(cfg: &Config) -> bool {
    cfg.uv().is_file()
}
fn comfy_ready(cfg: &Config) -> bool {
    cfg.comfy_dir().join("main.py").is_file()
}
fn venv_ready(cfg: &Config) -> bool {
    cfg.python().is_file()
}
fn deps_ready(cfg: &Config) -> bool {
    cfg.deps_marker().is_file()
}
fn model_ready(cfg: &Config, m: &crate::config::ModelFile) -> bool {
    m.path(&cfg.models_dir)
        .metadata()
        .map(|md| md.len() == m.size)
        .unwrap_or(false)
}

/// Free space on the volume that holds `path` (walks up to an existing ancestor).
pub fn free_space(path: &Path) -> Option<u64> {
    let existing = path.ancestors().find(|p| p.exists())?;
    fs4::available_space(existing).ok()
}

/// Picks the configured GPU, or the largest eligible one when none is saved.
fn select_gpu<'a>(cfg: &Config, gpus: &'a [gpu::Gpu]) -> Result<&'a gpu::Gpu, SetupError> {
    if gpus.is_empty() {
        return Err(SetupError::NoNvidiaGpu);
    }
    match cfg.gpu_selection.uuid.as_deref() {
        Some(uuid) => {
            gpus.iter()
                .find(|gpu| gpu.uuid == uuid)
                .ok_or_else(|| SetupError::SelectedGpuMissing {
                    uuid: uuid.to_string(),
                })
        }
        None => gpus
            .iter()
            .filter(|gpu| gpu_profile::recommend_pending(gpu).supported)
            .max_by(|left, right| {
                left.total_mb
                    .cmp(&right.total_mb)
                    .then_with(|| right.uuid.cmp(&left.uuid))
            })
            .or_else(|| {
                gpus.iter().max_by(|left, right| {
                    left.total_mb
                        .cmp(&right.total_mb)
                        .then_with(|| right.uuid.cmp(&left.uuid))
                })
            })
            .ok_or(SetupError::NoNvidiaGpu),
    }
}

fn profile_for(
    cfg: &Config,
    selected: &gpu::Gpu,
    recommendation: &GpuRecommendation,
) -> Result<Profile, SetupError> {
    match cfg.gpu_selection.profile_mode {
        ProfileMode::Auto => recommendation
            .profile
            .ok_or_else(|| SetupError::UnsupportedGpu {
                uuid: selected.uuid.clone(),
                reason: GpuSupportReason::InsufficientVram {
                    minimum_mb: 10_000,
                    actual_mb: selected.total_mb,
                },
            }),
        ProfileMode::Manual => Ok(cfg.profile),
    }
}

/// Eligibility for installing prerequisites and probing: driver and VRAM only. This never
/// proves architecture support, so it must not be used to plan or download model weights.
pub fn resolve_prerequisites(cfg: &Config, gpus: &[gpu::Gpu]) -> Result<ResolvedSetup, SetupError> {
    let selected = select_gpu(cfg, gpus)?;
    let recommendation = gpu_profile::recommend_pending(selected);
    if let Some(reason) = recommendation.reason {
        return Err(SetupError::UnsupportedGpu {
            uuid: selected.uuid.clone(),
            reason,
        });
    }
    Ok(ResolvedSetup {
        gpu: selected.clone(),
        profile: profile_for(cfg, selected, &recommendation)?,
        recommendation,
    })
}

/// Full eligibility including a runtime-proven capability. Without one this fails with
/// `UnknownCapability`, which is what keeps model weights from being planned or downloaded.
pub fn resolve_setup(
    cfg: &Config,
    gpus: &[gpu::Gpu],
    capability: Option<&GpuCapability>,
) -> Result<ResolvedSetup, SetupError> {
    let selected = select_gpu(cfg, gpus)?;
    let recommendation = gpu_profile::recommend(selected, capability);
    if let Some(reason) = recommendation.reason {
        return Err(SetupError::UnsupportedGpu {
            uuid: selected.uuid.clone(),
            reason,
        });
    }
    Ok(ResolvedSetup {
        gpu: selected.clone(),
        profile: profile_for(cfg, selected, &recommendation)?,
        recommendation,
    })
}

pub fn status(cfg: &Config) -> Status {
    match gpu::detect_all() {
        Ok(gpus) => {
            let proof = select_gpu(cfg, &gpus)
                .ok()
                .and_then(|selected| capability::load_proof(cfg, &selected.uuid));
            status_for_inventory(cfg, &gpus, proof.as_ref())
        }
        Err(error) => status_with_error(
            cfg,
            Vec::new(),
            SetupError::DiscoveryFailed {
                message: error.to_string(),
            },
            CapabilityState::Pending,
        ),
    }
}

pub fn status_for_inventory(
    cfg: &Config,
    gpus: &[gpu::Gpu],
    capability: Option<&GpuCapability>,
) -> Status {
    let devices = gpus
        .iter()
        .cloned()
        .map(|gpu| GpuSetupDevice {
            recommendation: gpu_profile::recommend_pending(&gpu),
            gpu,
        })
        .collect::<Vec<_>>();
    let state = CapabilityState::from_capability(capability);
    match capability {
        Some(_) => match resolve_setup(cfg, gpus, capability) {
            Ok(resolved) => status_for_resolved(cfg, devices, resolved, state),
            Err(error) => status_with_error(cfg, devices, error, state),
        },
        // No proof yet: show prerequisites only. Model steps stay hidden until the probe passes.
        None => match resolve_prerequisites(cfg, gpus) {
            Ok(resolved) => status_pending(cfg, devices, resolved),
            Err(error) => status_with_error(cfg, devices, error, state),
        },
    }
}

fn base_steps(cfg: &Config) -> Vec<Step> {
    let mut steps = vec![
        Step {
            id: "uv".into(),
            label: format!("uv {UV_VERSION} (Python manager)"),
            done: uv_ready(cfg) || deps_ready(cfg),
            download_bytes: 0,
        },
        Step {
            id: "comfy".into(),
            label: "ComfyUI engine".into(),
            done: comfy_ready(cfg),
            download_bytes: 0,
        },
        Step {
            id: "python".into(),
            label: "Python 3.12".into(),
            done: venv_ready(cfg),
            download_bytes: 0,
        },
        Step {
            id: "deps".into(),
            label: "PyTorch (CUDA 13) + engine packages".into(),
            done: deps_ready(cfg),
            download_bytes: 0,
        },
        Step {
            id: "gpucheck".into(),
            label: "GPU capability check".into(),
            done: false,
            download_bytes: 0,
        },
    ];
    for s in steps.iter_mut().filter(|s| !s.done) {
        s.download_bytes = match s.id.as_str() {
            "gpucheck" => 0,
            "uv" => 18 << 20,
            "comfy" => 12 << 20,
            "python" => 30 << 20,
            _ => 3_400 << 20, // torch cu130 + deps
        };
    }
    steps
}

fn status_for_resolved(
    cfg: &Config,
    gpus: Vec<GpuSetupDevice>,
    resolved: ResolvedSetup,
    capability: CapabilityState,
) -> Status {
    let mut steps = base_steps(cfg);
    mark_gpucheck(&mut steps, capability);
    for m in resolved.profile.models() {
        let done = model_ready(cfg, &m);
        let have = m
            .path(&cfg.models_dir)
            .with_extension("safetensors.part")
            .metadata()
            .map(|x| x.len())
            .unwrap_or(0);
        steps.push(Step {
            id: format!("model:{}", m.name),
            label: format!("{} ({})", m.name, human_bytes(m.size)),
            done,
            download_bytes: if done { 0 } else { m.size.saturating_sub(have) },
        });
    }
    let mut problems = vec![];
    let download_bytes = steps.iter().map(|s| s.download_bytes).sum();
    let free_bytes = free_space(&cfg.models_dir);
    if let Some(free) = free_bytes {
        // Leave 2 GB of slack for pip temp files.
        if free < download_bytes + (2 << 30) {
            problems.push(format!(
                "Not enough disk space: need about {}, {} free.",
                human_bytes(download_bytes + (2 << 30)),
                human_bytes(free)
            ));
        }
    }
    Status {
        ready: steps.iter().all(|s| s.done) && problems.is_empty(),
        steps,
        gpu: Some(resolved.gpu.clone()),
        gpus,
        selected_uuid: Some(resolved.gpu.uuid),
        effective_profile: Some(resolved.profile),
        recommendation: Some(resolved.recommendation),
        eligibility_error: None,
        capability,
        engine_fingerprint: crate::verification::engine_fingerprint(),
        problems,
        download_bytes,
        free_bytes,
    }
}

fn mark_gpucheck(steps: &mut [Step], capability: CapabilityState) {
    if let Some(step) = steps.iter_mut().find(|step| step.id == "gpucheck") {
        step.done = matches!(capability, CapabilityState::Proven { .. });
    }
}

/// Prerequisites are eligible but the GPU is unproven: only the install/probe steps are listed.
fn status_pending(cfg: &Config, gpus: Vec<GpuSetupDevice>, resolved: ResolvedSetup) -> Status {
    let steps = base_steps(cfg);
    let download_bytes: u64 = steps.iter().map(|s| s.download_bytes).sum();
    let free_bytes = free_space(&cfg.models_dir);
    let mut problems = vec![];
    if let Some(free) = free_bytes {
        if free < download_bytes + (2 << 30) {
            problems.push(format!(
                "Not enough disk space: need about {}, {} free.",
                human_bytes(download_bytes + (2 << 30)),
                human_bytes(free)
            ));
        }
    }
    Status {
        ready: false,
        steps,
        gpu: Some(resolved.gpu.clone()),
        gpus,
        selected_uuid: Some(resolved.gpu.uuid),
        effective_profile: Some(resolved.profile),
        recommendation: Some(resolved.recommendation),
        eligibility_error: None,
        capability: CapabilityState::Pending,
        engine_fingerprint: crate::verification::engine_fingerprint(),
        problems,
        download_bytes,
        free_bytes,
    }
}

fn status_with_error(
    cfg: &Config,
    gpus: Vec<GpuSetupDevice>,
    error: SetupError,
    capability: CapabilityState,
) -> Status {
    let mut steps = base_steps(cfg);
    mark_gpucheck(&mut steps, capability);
    Status {
        ready: false,
        steps,
        gpu: None,
        gpus,
        selected_uuid: cfg.gpu_selection.uuid.clone(),
        effective_profile: None,
        recommendation: None,
        eligibility_error: Some(error),
        capability,
        engine_fingerprint: crate::verification::engine_fingerprint(),
        problems: Vec::new(),
        download_bytes: 0,
        free_bytes: free_space(&cfg.models_dir),
    }
}

/// Runs a process and forwards each output line as a Log event.
fn run_logged(mut cmd: Command, emit: &mut dyn FnMut(Event)) -> Result<(), String> {
    no_window(&mut cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not start {:?}: {e}", cmd.get_program()))?;
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    // stderr on its own thread: reading it after stdout can deadlock when the pipe fills.
    let err = child.stderr.take().map(|e| {
        let tx = tx.clone();
        std::thread::spawn(move || {
            BufReader::new(e)
                .lines()
                .map_while(Result::ok)
                .for_each(|l| {
                    let _ = tx.send(l);
                })
        })
    });
    let out = child.stdout.take().map(|o| {
        let tx = tx.clone();
        std::thread::spawn(move || {
            BufReader::new(o)
                .lines()
                .map_while(Result::ok)
                .for_each(|l| {
                    let _ = tx.send(l);
                })
        })
    });
    drop(tx);
    let mut tail: Vec<String> = vec![];
    for line in rx {
        if !line.trim().is_empty() {
            tail.push(line.clone());
            if tail.len() > 20 {
                tail.remove(0);
            }
            emit(Event::Log { line });
        }
    }
    for h in [err, out].into_iter().flatten() {
        let _ = h.join();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{:?} exited with {status}. Last output:\n{}",
            cmd.get_program(),
            tail.join("\n")
        ))
    }
}

fn uv_cmd(cfg: &Config) -> Command {
    let mut c = Command::new(cfg.uv());
    // Keep Python and the package cache inside the runtime folder so uninstall = delete one folder.
    c.env("UV_PYTHON_INSTALL_DIR", cfg.runtime_dir.join("python"))
        .env("UV_CACHE_DIR", cfg.runtime_dir.join("cache"))
        .env("UV_NO_PROGRESS", "1")
        .env("UV_LINK_MODE", "copy");
    c
}

pub fn write_extra_paths(cfg: &Config) -> Result<(), String> {
    let base = cfg.models_dir.to_string_lossy().replace('\\', "/");
    let yaml = format!(
        "# Generated by PicaLum QwenImage. Points ComfyUI at the model folder.\npicalum_qwenimage:\n  base_path: \"{base}\"\n  diffusion_models: diffusion_models\n  text_encoders: text_encoders\n  vae: vae\n"
    );
    std::fs::create_dir_all(&cfg.runtime_dir).map_err(|e| e.to_string())?;
    std::fs::write(cfg.extra_paths_yaml(), yaml).map_err(|e| e.to_string())
}

/// The capability gate. Any stored proof is dropped first, so a failed or rejected probe can
/// never leave an older "proven" result behind. Weights are planned only from the `Ok` value.
/// The inventory is re-read after the probe so driver and VRAM are current when profiling.
fn prove_gpu(
    cfg: &Config,
    selected_uuid: &str,
    probe: impl FnOnce() -> Result<capability::RuntimeProbe, String>,
    inventory: impl FnOnce() -> Result<Vec<gpu::Gpu>, String>,
) -> Result<(GpuCapability, ResolvedSetup), String> {
    capability::clear_proof(cfg);
    let probe = probe()?;
    let capability =
        capability::capability_from_probe(selected_uuid, &probe).map_err(|e| e.to_string())?;
    let gpus = inventory()?;
    let resolved = resolve_setup(cfg, &gpus, Some(&capability)).map_err(|e| e.to_string())?;
    capability::save_proof(cfg, selected_uuid, capability)?;
    Ok((capability, resolved))
}

/// Installs whatever is missing. Blocking: call it from a thread.
pub fn run(cfg: &Config, emit: &mut dyn FnMut(Event)) -> Result<(), String> {
    let gpus = gpu::detect_all().map_err(|error| {
        SetupError::DiscoveryFailed {
            message: error.to_string(),
        }
        .to_string()
    })?;
    // Prerequisites only: the model list is planned after the runtime probe proves the GPU.
    let prerequisites = resolve_prerequisites(cfg, &gpus).map_err(|error| error.to_string())?;
    let mut effective_config = cfg.clone();
    effective_config.gpu_selection.uuid = Some(prerequisites.gpu.uuid);
    let cfg = &effective_config;
    let step = |emit: &mut dyn FnMut(Event), id: &str, state: &'static str, detail: String| {
        emit(Event::Step {
            id: id.into(),
            state,
            detail,
        })
    };
    std::fs::create_dir_all(&cfg.runtime_dir)
        .map_err(|e| format!("cannot create {}: {e}", cfg.runtime_dir.display()))?;
    let tmp = cfg.runtime_dir.join("tmp");
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;

    // 1. uv
    if uv_ready(cfg) || deps_ready(cfg) {
        step(emit, "uv", "skipped", "already installed".into());
    } else {
        step(emit, "uv", "running", "downloading".into());
        let zip = tmp.join("uv.zip");
        let url = format!("https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/uv-x86_64-pc-windows-msvc.zip");
        download::fetch(&url, &zip, None, &mut |p| {
            emit(Event::Progress {
                id: "uv".into(),
                done: p.done,
                total: p.total,
                bytes_per_sec: p.bytes_per_sec,
            })
        })?;
        download::unzip(&zip, &cfg.runtime_dir.join("bin"), false)?;
        let _ = std::fs::remove_file(&zip);
        step(emit, "uv", "done", String::new());
    }

    // 2. ComfyUI at the pinned commit
    if comfy_ready(cfg) {
        step(emit, "comfy", "skipped", "already installed".into());
    } else {
        step(
            emit,
            "comfy",
            "running",
            format!("commit {}", &COMFY_COMMIT[..8]),
        );
        let zip = tmp.join("comfyui.zip");
        let url = format!("https://codeload.github.com/comfyanonymous/ComfyUI/zip/{COMFY_COMMIT}");
        download::fetch(&url, &zip, None, &mut |p| {
            emit(Event::Progress {
                id: "comfy".into(),
                done: p.done,
                total: p.total,
                bytes_per_sec: p.bytes_per_sec,
            })
        })?;
        download::unzip(&zip, &cfg.comfy_dir(), true)?;
        let _ = std::fs::remove_file(&zip);
        step(emit, "comfy", "done", String::new());
    }

    // 3. Python venv
    if venv_ready(cfg) {
        step(emit, "python", "skipped", "already installed".into());
    } else {
        step(emit, "python", "running", "creating environment".into());
        let mut c = uv_cmd(cfg);
        c.current_dir(cfg.comfy_dir())
            .args(["venv", "--python", "3.12", "--seed", ".venv"]);
        run_logged(c, emit)?;
        step(emit, "python", "done", String::new());
    }

    // 4. PyTorch cu130 + ComfyUI requirements
    if deps_ready(cfg) {
        step(emit, "deps", "skipped", "already installed".into());
    } else {
        step(emit, "deps", "running", "PyTorch CUDA 13 (~3 GB)".into());
        let py = cfg.python();
        let mut c = uv_cmd(cfg);
        c.args(["pip", "install", "--python"]).arg(&py).args([
            "torch",
            "torchvision",
            "torchaudio",
            "--index-url",
            TORCH_INDEX,
        ]);
        run_logged(c, emit)?;
        step(emit, "deps", "running", "engine packages".into());
        let mut c = uv_cmd(cfg);
        c.args(["pip", "install", "--python"])
            .arg(&py)
            .arg("-r")
            .arg(cfg.comfy_dir().join("requirements.txt"));
        run_logged(c, emit)?;
        step(emit, "deps", "running", "checking CUDA".into());
        let mut c = Command::new(&py);
        c.args(["-c", "import torch;assert torch.cuda.is_available(),'CUDA not available';print('torch',torch.__version__,'cuda',torch.version.cuda,torch.cuda.get_device_name(),'identity-unverified')"]);
        run_logged(c, emit)?;
        std::fs::write(cfg.deps_marker(), COMFY_COMMIT).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(cfg.runtime_dir.join("cache")); // ~3 GB of wheels we no longer need
        step(emit, "deps", "done", String::new());
    }

    // 5. Runtime capability gate: nothing below downloads until the selected GPU is proven.
    let selected_uuid = cfg
        .gpu_selection
        .uuid
        .clone()
        .ok_or("No selected GPU UUID is configured.")?;
    step(emit, "gpucheck", "running", "probing PyTorch".into());
    let proven = prove_gpu(
        cfg,
        &selected_uuid,
        || capability::run_probe(cfg, &selected_uuid),
        || gpu::detect_all().map_err(|e| e.to_string()),
    );
    let (capability, resolved) = match proven {
        Ok(ok) => ok,
        Err(error) => {
            step(emit, "gpucheck", "error", error.clone());
            return Err(error);
        }
    };
    step(
        emit,
        "gpucheck",
        "done",
        format!(
            "compute capability {}.{}",
            capability.major, capability.minor
        ),
    );
    let mut effective_config = effective_config.clone();
    effective_config.profile = resolved.profile;
    let cfg = &effective_config;

    // 6. Weights
    for m in cfg.profile.models() {
        let id = format!("model:{}", m.name);
        let dest = m.path(&cfg.models_dir);
        if model_ready(cfg, &m) {
            step(emit, &id, "skipped", "already downloaded".into());
            continue;
        }
        step(emit, &id, "running", human_bytes(m.size));
        download::fetch(&m.url(), &dest, Some(m.size), &mut |p| {
            emit(Event::Progress {
                id: id.clone(),
                done: p.done,
                total: p.total,
                bytes_per_sec: p.bytes_per_sec,
            })
        })?;
        step(emit, &id, "running", "verifying checksum".into());
        let sum = download::sha256(&dest, &mut |p| {
            emit(Event::Progress {
                id: id.clone(),
                done: p.done,
                total: p.total,
                bytes_per_sec: p.bytes_per_sec,
            })
        })?;
        if sum != m.sha256 {
            let _ = std::fs::remove_file(&dest);
            return Err(format!(
                "{} failed its checksum and was deleted; run setup again.",
                m.name
            ));
        }
        step(emit, &id, "done", String::new());
    }

    write_extra_paths(cfg)?;
    let _ = std::fs::remove_dir_all(&tmp);
    cfg.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{resolve_setup, status_for_inventory, SetupError};
    use crate::config::{Config, Profile, ProfileMode, TE_INT8, TE_W4A8};
    use crate::gpu::Gpu;
    use crate::gpu_profile::{GpuCapability, GpuSupportReason};

    fn gpu(uuid: &str, total_mb: u32) -> Gpu {
        Gpu {
            uuid: uuid.to_string(),
            pci_bus_id: "00000000:01:00.0".to_string(),
            name: format!("NVIDIA {uuid}"),
            total_mb,
            used_mb: 0,
            util: 0,
            driver: "610.88".to_string(),
        }
    }

    fn config(uuid: Option<&str>, profile_mode: ProfileMode, profile: Profile) -> Config {
        let mut config = Config::default();
        config.gpu_selection.uuid = uuid.map(str::to_string);
        config.gpu_selection.profile_mode = profile_mode;
        config.profile = profile;
        config
    }

    #[test]
    fn rejects_setup_when_no_nvidia_gpu_is_detected() {
        let result = resolve_setup(
            &Config::default(),
            &[],
            Some(&GpuCapability {
                major: 8,
                minor: 6,
                compatible: true,
            }),
        );

        assert_eq!(result.unwrap_err(), SetupError::NoNvidiaGpu);
    }

    #[test]
    fn selects_largest_supported_gpu_then_lowest_uuid_when_saved_uuid_is_absent() {
        let gpus = vec![
            gpu("GPU-z", 12_000),
            gpu("GPU-b", 16_000),
            gpu("GPU-a", 16_000),
            gpu("GPU-small", 9_999),
        ];

        let resolved = resolve_setup(
            &Config::default(),
            &gpus,
            Some(&GpuCapability {
                major: 8,
                minor: 6,
                compatible: true,
            }),
        )
        .unwrap();

        assert_eq!(resolved.gpu.uuid, "GPU-a");
        assert_eq!(resolved.profile, Profile::Balanced);
    }

    #[test]
    fn rejects_a_saved_uuid_that_is_absent_from_fresh_inventory() {
        let config = config(Some("GPU-missing"), ProfileMode::Auto, Profile::Balanced);

        let result = resolve_setup(
            &config,
            &[gpu("GPU-present", 12_000)],
            Some(&GpuCapability {
                major: 8,
                minor: 6,
                compatible: true,
            }),
        );

        assert_eq!(
            result.unwrap_err(),
            SetupError::SelectedGpuMissing {
                uuid: "GPU-missing".to_string(),
            }
        );
    }

    #[test]
    fn rejects_selected_gpu_below_the_ten_gb_floor() {
        let config = config(Some("GPU-a"), ProfileMode::Manual, Profile::Compact);

        let result = resolve_setup(
            &config,
            &[gpu("GPU-a", 9_999)],
            Some(&GpuCapability {
                major: 8,
                minor: 6,
                compatible: true,
            }),
        );

        assert!(matches!(
            result,
            Err(SetupError::UnsupportedGpu {
                reason: GpuSupportReason::InsufficientVram {
                    minimum_mb: 10_000,
                    actual_mb: 9_999,
                },
                ..
            })
        ));
    }

    #[test]
    fn rejects_selected_gpu_below_the_driver_floor() {
        let mut old_driver = gpu("GPU-a", 24_000);
        old_driver.driver = "579.99".to_string();
        let config = config(Some("GPU-a"), ProfileMode::Auto, Profile::Balanced);

        let result = resolve_setup(
            &config,
            &[old_driver],
            Some(&GpuCapability {
                major: 8,
                minor: 6,
                compatible: true,
            }),
        );

        assert!(matches!(
            result,
            Err(SetupError::UnsupportedGpu {
                reason: GpuSupportReason::DriverTooOld {
                    minimum_major: 580,
                    actual_major: 579,
                },
                ..
            })
        ));
    }

    #[test]
    fn rejects_selected_gpu_with_explicitly_incompatible_capability() {
        let capability = GpuCapability {
            major: 6,
            minor: 1,
            compatible: false,
        };
        let config = config(Some("GPU-a"), ProfileMode::Auto, Profile::Balanced);

        let result = resolve_setup(&config, &[gpu("GPU-a", 24_000)], Some(&capability));

        assert!(matches!(
            result,
            Err(SetupError::UnsupportedGpu {
                reason: GpuSupportReason::IncompatibleCapability { major: 6, minor: 1 },
                ..
            })
        ));
    }

    #[test]
    fn automatic_mode_recomputes_compact_and_balanced_thresholds() {
        let config = config(Some("GPU-a"), ProfileMode::Auto, Profile::Balanced);
        let compact = resolve_setup(
            &config,
            &[gpu("GPU-a", 10_000)],
            Some(&GpuCapability {
                major: 8,
                minor: 6,
                compatible: true,
            }),
        )
        .unwrap();
        let balanced = resolve_setup(
            &config,
            &[gpu("GPU-a", 12_000)],
            Some(&GpuCapability {
                major: 8,
                minor: 6,
                compatible: true,
            }),
        )
        .unwrap();

        assert_eq!(compact.profile, Profile::Compact);
        assert_eq!(balanced.profile, Profile::Balanced);
    }

    #[test]
    fn manual_mode_retains_profile_after_hardware_validation() {
        let config = config(Some("GPU-a"), ProfileMode::Manual, Profile::Balanced);

        let resolved = resolve_setup(
            &config,
            &[gpu("GPU-a", 10_000)],
            Some(&GpuCapability {
                major: 8,
                minor: 6,
                compatible: true,
            }),
        )
        .unwrap();

        assert_eq!(resolved.profile, Profile::Balanced);
    }

    #[test]
    fn busy_vram_does_not_change_hardware_eligibility() {
        let mut busy_gpu = gpu("GPU-a", 10_000);
        busy_gpu.used_mb = 9_900;
        let config = config(Some("GPU-a"), ProfileMode::Auto, Profile::Balanced);

        let resolved = resolve_setup(
            &config,
            &[busy_gpu],
            Some(&GpuCapability {
                major: 8,
                minor: 6,
                compatible: true,
            }),
        )
        .unwrap();

        assert_eq!(resolved.profile, Profile::Compact);
    }

    #[test]
    fn status_model_steps_use_only_the_effective_profile_encoder() {
        let auto = config(Some("GPU-a"), ProfileMode::Auto, Profile::Balanced);
        let compact = status_for_inventory(
            &auto,
            &[gpu("GPU-a", 10_000)],
            Some(&GpuCapability {
                major: 8,
                minor: 6,
                compatible: true,
            }),
        );
        let balanced = status_for_inventory(
            &auto,
            &[gpu("GPU-a", 12_000)],
            Some(&GpuCapability {
                major: 8,
                minor: 6,
                compatible: true,
            }),
        );
        let compact_ids: Vec<&str> = compact.steps.iter().map(|step| step.id.as_str()).collect();
        let balanced_ids: Vec<&str> = balanced.steps.iter().map(|step| step.id.as_str()).collect();

        assert!(compact_ids.contains(&format!("model:{}", TE_W4A8.name).as_str()));
        assert!(!compact_ids.contains(&format!("model:{}", TE_INT8.name).as_str()));
        assert!(balanced_ids.contains(&format!("model:{}", TE_INT8.name).as_str()));
        assert!(!balanced_ids.contains(&format!("model:{}", TE_W4A8.name).as_str()));
    }
}

#[cfg(test)]
mod capability_gate_tests {
    use super::{resolve_prerequisites, resolve_setup, status_for_inventory, SetupError};
    use crate::capability::CapabilityState;
    use crate::config::{Config, Profile, ProfileMode};
    use crate::gpu::Gpu;
    use crate::gpu_profile::{GpuCapability, GpuSupportReason};

    fn gpu(uuid: &str, total_mb: u32) -> Gpu {
        Gpu {
            uuid: uuid.into(),
            pci_bus_id: "00000000:01:00.0".into(),
            name: "NVIDIA GPU".into(),
            total_mb,
            used_mb: 0,
            util: 0,
            driver: "610.88".into(),
        }
    }

    fn cap(compatible: bool) -> GpuCapability {
        GpuCapability {
            major: 8,
            minor: 6,
            compatible,
        }
    }

    fn model_steps(status: &super::Status) -> usize {
        status
            .steps
            .iter()
            .filter(|s| s.id.starts_with("model:"))
            .count()
    }

    #[test]
    fn unproven_gpu_is_pending_with_prerequisites_and_no_model_steps() {
        let status = status_for_inventory(&Config::default(), &[gpu("GPU-a", 12_000)], None);
        assert_eq!(status.capability, CapabilityState::Pending);
        assert!(
            status.eligibility_error.is_none(),
            "setup must not be blocked"
        );
        assert!(!status.ready);
        assert_eq!(model_steps(&status), 0);
        assert!(status.steps.iter().any(|s| s.id == "gpucheck" && !s.done));
        assert_eq!(status.effective_profile, Some(Profile::Balanced));
    }

    #[test]
    fn model_planning_requires_a_proven_capability() {
        let config = Config::default();
        let gpus = [gpu("GPU-a", 12_000)];
        let result = resolve_setup(&config, &gpus, None);
        assert!(matches!(
            result,
            Err(SetupError::UnsupportedGpu {
                reason: GpuSupportReason::UnknownCapability,
                ..
            })
        ));
        let proven = status_for_inventory(&config, &gpus, Some(&cap(true)));
        assert_eq!(
            proven.capability,
            CapabilityState::Proven { major: 8, minor: 6 }
        );
        assert_eq!(model_steps(&proven), 3);
        assert!(proven.steps.iter().any(|s| s.id == "gpucheck" && s.done));
    }

    #[test]
    fn rejected_capability_blocks_without_model_steps() {
        let status = status_for_inventory(
            &Config::default(),
            &[gpu("GPU-a", 24_000)],
            Some(&cap(false)),
        );
        assert_eq!(
            status.capability,
            CapabilityState::Rejected { major: 8, minor: 6 }
        );
        assert!(matches!(
            status.eligibility_error,
            Some(SetupError::UnsupportedGpu {
                reason: GpuSupportReason::IncompatibleCapability { .. },
                ..
            })
        ));
        assert_eq!(model_steps(&status), 0);
        assert!(!status.ready);
    }

    #[test]
    fn prerequisites_still_enforce_vram_floor_and_driver() {
        let small = resolve_prerequisites(&Config::default(), &[gpu("GPU-a", 9_999)]);
        assert!(matches!(
            small,
            Err(SetupError::UnsupportedGpu {
                reason: GpuSupportReason::InsufficientVram { .. },
                ..
            })
        ));
        let mut old = gpu("GPU-b", 24_000);
        old.driver = "575.1".into();
        assert!(matches!(
            resolve_prerequisites(&Config::default(), &[old]),
            Err(SetupError::UnsupportedGpu {
                reason: GpuSupportReason::DriverTooOld { .. },
                ..
            })
        ));
    }

    #[test]
    fn manual_profile_survives_pending_and_auto_follows_vram() {
        let gpus = [gpu("GPU-a", 24_000)];
        let mut manual = Config::default();
        manual.gpu_selection.profile_mode = ProfileMode::Manual;
        manual.profile = Profile::Compact;
        assert_eq!(
            status_for_inventory(&manual, &gpus, None).effective_profile,
            Some(Profile::Compact)
        );
        let mut auto = Config {
            profile: Profile::Compact,
            ..Config::default()
        };
        auto.gpu_selection.profile_mode = ProfileMode::Auto;
        assert_eq!(
            status_for_inventory(&auto, &gpus, None).effective_profile,
            Some(Profile::Balanced)
        );
    }
}

#[cfg(test)]
mod prove_gpu_tests {
    use super::prove_gpu;
    use crate::capability::{load_proof, save_proof, RuntimeProbe};
    use crate::config::Config;
    use crate::gpu::Gpu;
    use crate::gpu_profile::GpuCapability;
    use std::cell::Cell;

    fn gpu(uuid: &str, total_mb: u32) -> Gpu {
        Gpu {
            uuid: uuid.into(),
            pci_bus_id: "00000000:01:00.0".into(),
            name: "NVIDIA GPU".into(),
            total_mb,
            used_mb: 0,
            util: 0,
            driver: "610.88".into(),
        }
    }

    fn probe(uuid: &str, major: u32, archs: &[&str]) -> RuntimeProbe {
        RuntimeProbe {
            uuid: uuid.into(),
            major,
            minor: 6,
            arch_list: archs.iter().map(|a| a.to_string()).collect(),
        }
    }

    /// A config with an installed runtime and a stale "proven" result already on disk.
    fn config_with_stale_proof() -> (tempfile::TempDir, Config) {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config {
            runtime_dir: dir.path().to_path_buf(),
            ..Config::default()
        };
        cfg.gpu_selection.uuid = Some("GPU-aaaa".into());
        std::fs::write(cfg.deps_marker(), "commit").unwrap();
        let stale = GpuCapability {
            major: 8,
            minor: 6,
            compatible: true,
        };
        save_proof(&cfg, "GPU-aaaa", stale).unwrap();
        assert_eq!(load_proof(&cfg, "GPU-aaaa"), Some(stale));
        (dir, cfg)
    }

    fn assert_stopped_before_inventory(probe_result: Result<RuntimeProbe, String>, expected: &str) {
        let (_dir, cfg) = config_with_stale_proof();
        let inventory_read = Cell::new(false);
        let result = prove_gpu(
            &cfg,
            "GPU-aaaa",
            || probe_result,
            || {
                inventory_read.set(true);
                Ok(vec![gpu("GPU-aaaa", 12_000)])
            },
        );
        let error = result.unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(
            !inventory_read.get(),
            "no profile planning after a failed probe"
        );
        assert_eq!(
            load_proof(&cfg, "GPU-aaaa"),
            None,
            "stale proof must be gone"
        );
    }

    #[test]
    fn failed_probe_stops_the_gate_and_clears_the_old_proof() {
        assert_stopped_before_inventory(Err("probe timed out".into()), "timed out");
    }

    #[test]
    fn mismatched_runtime_uuid_stops_the_gate() {
        assert_stopped_before_inventory(Ok(probe("GPU-bbbb", 8, &["sm_80"])), "different GPU");
    }

    #[test]
    fn missing_architecture_list_stops_the_gate() {
        assert_stopped_before_inventory(Ok(probe("GPU-aaaa", 8, &[])), "architectures");
    }

    #[test]
    fn unsupported_architecture_is_rejected_and_not_persisted_as_proven() {
        let (_dir, cfg) = config_with_stale_proof();
        let result = prove_gpu(
            &cfg,
            "GPU-aaaa",
            || Ok(probe("GPU-aaaa", 6, &["sm_80", "sm_90"])),
            || Ok(vec![gpu("GPU-aaaa", 12_000)]),
        );
        assert!(result.unwrap_err().contains("compute capability"));
        assert_eq!(load_proof(&cfg, "GPU-aaaa"), None);
    }

    #[test]
    fn proven_gpu_returns_the_profile_and_persists_the_proof_from_a_fresh_inventory() {
        let (_dir, cfg) = config_with_stale_proof();
        let (capability, resolved) = prove_gpu(
            &cfg,
            "GPU-aaaa",
            || Ok(probe("GPU-aaaa", 8, &["sm_80"])),
            || Ok(vec![gpu("GPU-aaaa", 10_000)]),
        )
        .unwrap();
        assert!(capability.compatible);
        assert_eq!(resolved.profile, crate::config::Profile::Compact);
        assert_eq!(load_proof(&cfg, "GPU-aaaa"), Some(capability));
    }

    #[test]
    fn selected_gpu_vanishing_after_the_probe_stops_the_gate() {
        let (_dir, cfg) = config_with_stale_proof();
        let result = prove_gpu(
            &cfg,
            "GPU-aaaa",
            || Ok(probe("GPU-aaaa", 8, &["sm_80"])),
            || Ok(vec![gpu("GPU-other", 12_000)]),
        );
        assert!(result.unwrap_err().contains("not present"));
        assert_eq!(load_proof(&cfg, "GPU-aaaa"), None);
    }
}
