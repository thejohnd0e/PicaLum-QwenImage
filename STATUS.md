# Project Status

## NVIDIA auto-profile and verification: merged into `main` (build 0.1.15)

Plan: [`docs/superpowers/plans/2026-10-09-nvidia-auto-profile-verification.md`](docs/superpowers/plans/2026-10-09-nvidia-auto-profile-verification.md)

Core merged as commit `aea1b1c`; later UI changes (0.1.11-0.1.15) are in the following commit. The temporary worktree `C:\Users\la\AppData\Local\Temp\opencode\picalum-nvidia-task1` (branch `task/nvidia-auto-profile-task1`) is no longer needed and can be removed with `git worktree remove`.

### Done

- GPU inventory by stable UUID; shared Compact/Balanced policy (10,000 MB floor, Balanced from 12,000 MB); selection bound to the ComfyUI child and telemetry.
- Runtime capability gate (Task 3): prerequisites -> PyTorch probe under the selected UUID (runtime UUID match, compiled `sm_XY` list) -> only then model planning/download. Pending / proven / rejected in desktop and `qil doctor`. Independently reviewed (PASS) and the review fixes applied.
- Draft verification with isolated output, cancel/retry, localized UI. It is no longer automatic (0.1.11): it runs on demand from Settings → Check GPU. Card styled, buttons labelled, real error shown; the setup status exposes the engine fingerprint.
- Header (0.1.12-0.1.15): product name on top, version and GitHub link on one line below, larger name, larger Create/Edit/Surprise tabs centered between the brand and the status controls; model name and profile tag removed.
- Build 0.1.15 (version synchronized in `Cargo.toml`, `Cargo.lock`, `app/package.json`, `app/package-lock.json`, `tauri.conf.json`, CHANGELOG). Installer: `out/PicaLum QwenImage_0.1.15_x64-setup.exe`.
- Checks at build: `cargo test --workspace` (112 core), `node --test ui/tests/*.test.js` (45), `cargo fmt --all -- --check`.

### Verified on hardware

RTX 3060 12 GB (driver 580+): capability probe proven 8.6, Draft generation, verification passed in about 61 s with 11,124 MB peak VRAM, result persisted and shown by `qil doctor`; the desktop verification card, including progress, works (verified on 0.1.10; the check is now manual).

### Not verified

- Other GPUs: 10-12 GB (Compact), pre-Ampere/unsupported architecture, multi-GPU selection.
- Skip/Cancel/failure/OOM paths in the live app (covered by tests only).
- No final independent review of the whole release; only Task 3 and Tasks 4-7 were reviewed separately.

### Known limitations

- `gpu_capability.json` is a local cache; `setup::run` always re-probes and never trusts it for downloads. `qil generate` does not itself require a proof. CLI "to download" excludes model bytes while pending.
- The main checkout still has unrelated uncommitted user changes: `AGENTS.md`, deleted `assets/*`, and untracked notes and plans. Do not reset or clean.
