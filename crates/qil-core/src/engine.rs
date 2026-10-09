//! Engine lifecycle: a headless ComfyUI running Qwen-Image-2.1.
//!
//! If something already answers on the port we reuse it. Otherwise we launch our own,
//! without a console window, inside a Windows Job Object that kills it when this
//! process exits (even on a crash), so VRAM is always released.

use std::fs::File;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;

use crate::config::Config;
use crate::gpu::no_window;
use crate::setup;

#[derive(Debug, Clone, Serialize)]
pub struct EngineState {
    pub state: &'static str, // off | starting | ready | error
    pub detail: String,
    pub external: bool,
}

#[derive(Default)]
pub struct Engine {
    child: Mutex<Option<Child>>,
    #[cfg(windows)]
    job: Mutex<Option<win32job::Job>>,
}

pub async fn is_up(cfg: &Config) -> bool {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
        .unwrap();
    matches!(client.get(format!("{}/system_stats", cfg.base_url())).send().await, Ok(r) if r.status().is_success())
}

impl Engine {
    pub fn owns_process(&self) -> bool {
        self.child.lock().unwrap().is_some()
    }

    /// Starts the engine (or attaches to a running one) and waits until it answers.
    pub async fn start(
        &self,
        cfg: &Config,
        on_state: &(dyn Fn(EngineState) + Send + Sync),
    ) -> Result<(), String> {
        let send = |state, detail: String, external| {
            on_state(EngineState {
                state,
                detail,
                external,
            })
        };
        if is_up(cfg).await {
            send("ready", "Engine ready".into(), !self.owns_process());
            return Ok(());
        }
        if !self.owns_process() {
            if let Err(e) = self.spawn(cfg) {
                send("error", e.clone(), false);
                return Err(e);
            }
        }
        send("starting", "Starting engine…".into(), false);
        // First start imports torch and the nodes: 15–40 s depending on the disk.
        for _ in 0..240 {
            tokio::time::sleep(Duration::from_millis(750)).await;
            if is_up(cfg).await {
                send("ready", "Engine ready".into(), false);
                return Ok(());
            }
            if let Some(code) = self.exited() {
                let msg = format!(
                    "The engine exited while starting (code {code}). See {}",
                    cfg.engine_log().display()
                );
                send("error", msg.clone(), false);
                return Err(msg);
            }
        }
        let msg = format!(
            "The engine did not answer within 3 minutes. See {}",
            cfg.engine_log().display()
        );
        send("error", msg.clone(), false);
        Err(msg)
    }

    fn spawn(&self, cfg: &Config) -> Result<(), String> {
        let st = setup::status(cfg);
        if !st.ready {
            return Err("The runtime is not installed yet. Run setup first (qil setup).".into());
        }
        setup::write_extra_paths(cfg)?;
        let log = File::create(cfg.engine_log()).map_err(|e| e.to_string())?;
        let log_err = log.try_clone().map_err(|e| e.to_string())?;
        let mut cmd = Command::new(cfg.python());
        cmd.current_dir(cfg.comfy_dir())
            .arg("-s")
            .arg("main.py")
            .args([
                "--listen",
                "127.0.0.1",
                "--port",
                &cfg.port.to_string(),
                "--disable-auto-launch",
            ])
            .arg("--extra-model-paths-config")
            .arg(cfg.extra_paths_yaml())
            .env("PYTHONUNBUFFERED", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_err));
        if let Ok(extra) = std::env::var("QIL_ENGINE_ARGS") {
            cmd.args(extra.split_whitespace()); // e.g. "--reserve-vram 4" to emulate a smaller card
        }
        apply_selected_gpu_env(&mut cmd, cfg)?;
        let runtime_uuid =
            probe_runtime_gpu_uuid(&cfg.python(), cfg.gpu_selection.uuid.as_deref())?;
        match check_runtime_gpu_identity(
            cfg.gpu_selection.uuid.as_deref().unwrap_or_default(),
            Some(&runtime_uuid),
        ) {
            GpuIdentityCheck::Match => {}
            GpuIdentityCheck::Mismatch => {
                return Err(format!(
                    "Selected GPU identity mismatch: runtime reported {runtime_uuid}."
                ))
            }
            GpuIdentityCheck::NotFound | GpuIdentityCheck::Unverified => {
                return Err("Selected GPU identity could not be verified by PyTorch.".into())
            }
        }
        let child = no_window(&mut cmd)
            .spawn()
            .map_err(|e| format!("Could not launch the engine: {e}"))?;

        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            if let Ok(job) = win32job::Job::create() {
                if let Ok(mut info) = job.query_extended_limit_info() {
                    info.limit_kill_on_job_close();
                    let _ = job.set_extended_limit_info(&info);
                }
                let _ = job.assign_process(child.as_raw_handle() as isize);
                *self.job.lock().unwrap() = Some(job);
            }
        }
        *self.child.lock().unwrap() = Some(child);
        Ok(())
    }

    fn exited(&self) -> Option<i32> {
        let mut guard = self.child.lock().unwrap();
        let child = guard.as_mut()?;
        match child.try_wait() {
            Ok(Some(status)) => {
                *guard = None;
                Some(status.code().unwrap_or(-1))
            }
            _ => None,
        }
    }

    /// Stops the engine if we launched it (frees the VRAM). Returns whether it did.
    pub fn stop(&self) -> bool {
        let taken = self.child.lock().unwrap().take();
        if let Some(mut child) = taken {
            let _ = child.kill();
            let _ = child.wait();
            #[cfg(windows)]
            {
                self.job.lock().unwrap().take();
            }
            return true;
        }
        false
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Applies selected GPU binding only to the ComfyUI child command.
pub fn apply_selected_gpu_env(cmd: &mut Command, cfg: &Config) -> Result<(), String> {
    let uuid = cfg
        .gpu_selection
        .uuid
        .as_deref()
        .ok_or("No selected GPU UUID is configured.")?;
    if uuid.trim().is_empty() {
        return Err("Selected GPU UUID is empty.".into());
    }
    cmd.env("CUDA_VISIBLE_DEVICES", uuid);
    Ok(())
}

#[cfg(test)]
mod task4_tests {
    use super::apply_selected_gpu_env;
    use crate::Config;
    use std::process::Command;

    #[test]
    fn selected_uuid_is_bound_only_on_child_command() {
        let mut cfg = Config::default();
        cfg.gpu_selection.uuid = Some("GPU-test-uuid".into());
        let mut command = Command::new("python");
        apply_selected_gpu_env(&mut command, &cfg).unwrap();
        assert_eq!(
            command
                .get_envs()
                .find(|(key, _)| *key == "CUDA_VISIBLE_DEVICES")
                .and_then(|(_, value)| value)
                .and_then(|v| v.to_str()),
            Some("GPU-test-uuid")
        );
    }

    #[test]
    fn missing_selected_uuid_is_explicit() {
        let mut command = Command::new("python");
        assert!(apply_selected_gpu_env(&mut command, &Config::default()).is_err());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuIdentityCheck {
    Match,
    Mismatch,
    NotFound,
    Unverified,
}

/// Classifies a runtime-reported UUID without inferring identity from model names or ordinals.
pub fn check_runtime_gpu_identity(expected: &str, runtime_uuid: Option<&str>) -> GpuIdentityCheck {
    let Some(runtime_uuid) = runtime_uuid else {
        return GpuIdentityCheck::Unverified;
    };
    if runtime_uuid.trim().is_empty() {
        return GpuIdentityCheck::NotFound;
    }
    let Some(expected) = normalize_torch_gpu_uuid(expected) else {
        return GpuIdentityCheck::Mismatch;
    };
    let Some(runtime_uuid) = normalize_torch_gpu_uuid(runtime_uuid) else {
        return GpuIdentityCheck::NotFound;
    };
    if runtime_uuid.eq_ignore_ascii_case(&expected) {
        GpuIdentityCheck::Match
    } else {
        GpuIdentityCheck::Mismatch
    }
}

#[cfg(test)]
mod identity_tests {
    use super::{check_runtime_gpu_identity, GpuIdentityCheck};

    #[test]
    fn accepts_matching_runtime_uuid() {
        assert_eq!(
            check_runtime_gpu_identity("GPU-a", Some("GPU-a")),
            GpuIdentityCheck::Match
        );
    }
    #[test]
    fn accepts_mixed_case_normalized_uuid() {
        assert_eq!(
            check_runtime_gpu_identity("gpu-{ABCDEF-01}", Some("GPU-abcdef-01")),
            GpuIdentityCheck::Match
        );
    }
    #[test]
    fn rejects_mismatched_runtime_uuid() {
        assert_eq!(
            check_runtime_gpu_identity("GPU-a", Some("GPU-b")),
            GpuIdentityCheck::Mismatch
        );
    }
    #[test]
    fn distinguishes_not_found_and_unverified() {
        assert_eq!(
            check_runtime_gpu_identity("GPU-a", Some("")),
            GpuIdentityCheck::NotFound
        );
        assert_eq!(
            check_runtime_gpu_identity("GPU-a", None),
            GpuIdentityCheck::Unverified
        );
    }
}

pub fn normalize_torch_gpu_uuid(raw: &str) -> Option<String> {
    let value = raw.trim().trim_matches(['"', '\'']);
    // Some torch builds print the bare UUID without NVIDIA's `GPU-` prefix.
    let bare = value.len() == 36
        && value.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        });
    let value = if bare {
        value
    } else {
        value
            .strip_prefix("GPU-")
            .or_else(|| value.strip_prefix("gpu-"))?
    };
    let compact = value
        .replace(['{', '}', ' ', '\\'], "")
        .to_ascii_lowercase();
    if compact.is_empty() || !compact.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return None;
    }
    Some(format!("GPU-{compact}"))
}

pub fn probe_runtime_gpu_uuid(
    python: &Path,
    selected_uuid: Option<&str>,
) -> Result<String, String> {
    let selected = selected_uuid.ok_or("No selected GPU UUID is configured.")?;
    let mut cmd = Command::new(python);
    cmd.args([
        "-c",
        "import torch; p=torch.cuda.get_device_properties(0); print(p.uuid)",
    ])
    .env("CUDA_VISIBLE_DEVICES", selected)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let output = no_window(&mut cmd)
        .output()
        .map_err(|e| format!("Could not run PyTorch GPU identity probe: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "PyTorch GPU identity probe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .rev()
        .find_map(normalize_torch_gpu_uuid)
        .ok_or_else(|| "PyTorch did not report a GPU UUID.".into())
}

#[cfg(test)]
mod torch_uuid_tests {
    use super::normalize_torch_gpu_uuid;
    #[test]
    fn normalizes_torch_uuid_format() {
        assert_eq!(
            normalize_torch_gpu_uuid("GPU-{ABCDEF-01}"),
            Some("GPU-abcdef-01".into())
        );
    }
    #[test]
    fn accepts_bare_uuid_without_gpu_prefix() {
        assert_eq!(
            normalize_torch_gpu_uuid("ABCDEF01-2345-6789-abcd-ef0123456789"),
            Some("GPU-abcdef01-2345-6789-abcd-ef0123456789".into())
        );
        assert_eq!(
            normalize_torch_gpu_uuid("abcdef01-2345-6789-abcd-ef012345678"),
            None
        );
    }
    #[test]
    fn rejects_missing_or_malformed_uuid() {
        assert_eq!(normalize_torch_gpu_uuid(""), None);
        assert_eq!(normalize_torch_gpu_uuid("RTX-3060"), None);
    }
}
