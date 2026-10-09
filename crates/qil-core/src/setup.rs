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

use crate::config::{Config, COMFY_COMMIT, MIN_DRIVER_MAJOR, TORCH_INDEX, UV_VERSION};
use crate::download::{self, human_bytes};
use crate::gpu::{self, no_window};

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
    pub problems: Vec<String>,
    pub download_bytes: u64,
    pub free_bytes: Option<u64>,
}

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

pub fn status(cfg: &Config) -> Status {
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
    ];
    for s in steps.iter_mut().filter(|s| !s.done) {
        s.download_bytes = match s.id.as_str() {
            "uv" => 18 << 20,
            "comfy" => 12 << 20,
            "python" => 30 << 20,
            _ => 3_400 << 20, // torch cu130 + deps
        };
    }
    for m in cfg.profile.models() {
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
    let gpu = gpu::detect();
    let mut problems = vec![];
    match &gpu {
        None => problems.push("No NVIDIA GPU detected (nvidia-smi not found). This release needs an NVIDIA card.".into()),
        Some(g) if g.driver_major() < MIN_DRIVER_MAJOR => problems.push(format!(
            "NVIDIA driver {} is too old: CUDA 13 needs {MIN_DRIVER_MAJOR} or newer. Update it from nvidia.com.",
            g.driver
        )),
        Some(g) if g.total_mb < 7_500 => problems.push(format!(
            "{} has {} GB of VRAM; the smallest supported profile needs 8 GB.",
            g.short_name(),
            g.total_mb / 1024
        )),
        _ => {}
    }
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
        ready: steps.iter().all(|s| s.done),
        steps,
        gpu,
        problems,
        download_bytes,
        free_bytes,
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

/// Installs whatever is missing. Blocking: call it from a thread.
pub fn run(cfg: &Config, emit: &mut dyn FnMut(Event)) -> Result<(), String> {
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
        c.args(["-c", "import torch;assert torch.cuda.is_available(),'CUDA not available';print('torch',torch.__version__,'cuda',torch.version.cuda,torch.cuda.get_device_name())"]);
        run_logged(c, emit)?;
        std::fs::write(cfg.deps_marker(), COMFY_COMMIT).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(cfg.runtime_dir.join("cache")); // ~3 GB of wheels we no longer need
        step(emit, "deps", "done", String::new());
    }

    // 5. Weights
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
