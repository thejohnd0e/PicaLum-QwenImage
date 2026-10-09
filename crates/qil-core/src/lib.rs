//! qil-core: installer, engine lifecycle and generation client for running
//! Qwen-Image-2.1 fully in VRAM on 8–12 GB NVIDIA GPUs.
//!
//! Shared by the desktop app (Tauri) and the `qil` command line.

pub mod config;
pub mod download;
pub mod engine;
pub mod generate;
pub mod gpu;
mod png_metadata;
pub mod run_control;
pub mod setup;
pub mod surprise;

pub use config::{Config, Profile};
pub use engine::{Engine, EngineState};
pub use generate::{GenRequest, GenResult, Progress};
