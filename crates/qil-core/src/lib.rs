//! qil-core: installer, engine lifecycle and generation client for running
//! Qwen-Image-2.1 fully in VRAM on 8–12 GB NVIDIA GPUs.
//!
//! Shared by the desktop app (Tauri) and the `qil` command line.

pub mod capability;
pub mod config;
pub mod download;
pub mod engine;
pub mod generate;
pub mod gpu;
pub mod gpu_profile;
pub mod masked_edit;
pub mod masked_run;
mod png_metadata;
pub mod prompt_presets;
pub mod run_control;
pub mod setup;
pub mod surprise;
pub mod verification;

pub use config::{Config, GpuVerification, Profile};
pub use engine::{Engine, EngineState};
pub use generate::{GenRequest, GenResult, Progress};
