# Repository guide

## Layout
- `crates/qil-core` owns shared setup/download, GPU detection, engine lifecycle, generation, and configuration; both front ends use it.
- `crates/qil-cli` is the `qil` CLI. `app/src-tauri` is the Tauri/Rust desktop backend; its plain HTML/CSS/JS UI is under `app/ui`.
- `bench/` contains benchmark suites, inputs/results, and the tier runner; `docs/` contains the benchmark site.

## Build and verify
- Root build: `./build.ps1` (PowerShell; add `-Installer` to produce the NSIS installer). It builds CLI first, then desktop app, copying binaries to `out/`.
- CLI only: `cargo build --release -p qil-cli`.
- Rust workspace tests: `cargo test --workspace`.
- Desktop app: from `app/`, run `npm ci` once, then `npm run dev` or `npm run build`.
- Benchmark tier run: `bash bench/tiers/run.sh`; it requires a configured local runtime/GPU and can generate result files.

## Project-specific notes
- Cargo workspace members are `crates/qil-core`, `crates/qil-cli`, and `app/src-tauri`; workspace dependency versions live in root `Cargo.toml`.
- Desktop UI assets are loaded from `app/ui/index.html`; Tauri commands and events are wired in `app/src-tauri/src/lib.rs`.
- Generation/setup uses the external ComfyUI process via `qil-core`; for changes spanning UI and CLI behavior, keep shared behavior in `qil-core` rather than duplicating it in a frontend.
- The real download integration test is `crates/qil-core/tests/download_real.rs`; inspect it before running because it may require network access.
- Do not treat `target/`, `out/`, or `app/node_modules/` as source; they are build/dependency outputs.
