//! Engine lifecycle: a headless ComfyUI running Qwen-Image-2.1.
//!
//! If something already answers on the port we reuse it. Otherwise we launch our own,
//! without a console window, inside a Windows Job Object that kills it when this
//! process exits (even on a crash), so VRAM is always released.

use std::fs::File;
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
