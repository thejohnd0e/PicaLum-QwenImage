# NVIDIA Auto-Profile and Installation Verification Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `executing-plans` to implement this plan task-by-task, with verification after each task.

**Goal:** Detect supported NVIDIA GPUs during setup, automatically recommend a VRAM profile for cards with at least 10 GB, bind the engine to the selected GPU, and run an optional-to-skip Draft verification generation after installation.

**Architecture:** Extend shared `qil-core` GPU discovery and profile selection, persist the selected GPU/profile and verification status in the existing config, and pass the selected GPU identity to the ComfyUI process. The Tauri setup flow presents the recommendation, then after installation runs a small fixed Draft generation in an isolated temporary output location; the user can skip, retry, or use the app with an unverified status.

**Tech Stack:** Rust workspace, serde/serde_json, `nvidia-smi`, Tauri 2, ComfyUI/PyTorch CUDA, plain HTML/CSS/JavaScript, Rust and Node.js tests.

**Spec:** Approved in the conversation: NVIDIA on Windows only; minimum 10 GB dedicated VRAM; Compact at 10 to <12 GB; Balanced at 12 GB or more; reject unsupported GPU/driver before model downloads; automatically run a short Draft verification after install with a Skip action; allow verification later; separate transient VRAM pressure from unsupported hardware; do not put verification outputs in user history.

## Global Constraints

- Supported scope is NVIDIA GPUs on Windows only.
- Minimum supported memory is 10 GB of dedicated VRAM; this is a product support floor, not a promise that all workflows fit in memory.
- Profile thresholds: Compact for 10 GB to <12 GB; Balanced for >=12 GB.
- Driver compatibility and supported compute architecture must be checked independently of VRAM.
- Current driver minimum for CUDA 13 is `MIN_DRIVER_MAJOR = 580`; keep one shared source of truth.
- Do not call a card unsupported just because other applications temporarily occupy VRAM.
- Bind engine execution, GPU status reporting, and verification measurements to the selected GPU, especially on multi-GPU machines.
- Verification uses Draft (1024x1024, 20 steps), a fixed prompt and deterministic seed; it proves this baseline request ran, not that Max, multi-image edits, or every workload will fit.
- Verification output must not enter normal output folders or session history; clean it up after recording results.
- Skip or failed verification must not block opening the app; status remains visible and can be retried later.
- Preserve backward compatibility when loading existing config files with no GPU-selection or verification fields.
- Do not run the ignored real-download integration test; it downloads large model files and is unrelated.
- Do not commit or publish unless separately requested.

---

## 1. Existing Project Context and File Map

Current implementation already has several foundations:

- `crates/qil-core/src/gpu.rs` invokes `nvidia-smi`, but parses only the first output row and does not expose stable device identity or enumerate all GPUs.
- `Profile::for_vram` currently uses an 11,000 MB boundary; the desktop setup has manual Balanced/Compact radio buttons.
- `setup::status` detects GPU and driver/VRAM problems. `setup::run` installs engine dependencies and profile model weights.
- Desktop setup commands in `app/src-tauri/src/lib.rs` configure and run setup; frontend completion is handled in `app/ui/app.js`.
- Engine spawn in `crates/qil-core/src/engine.rs` starts ComfyUI and currently does not bind it to a selected GPU.
- Desktop `gpu_stats` in `app/src-tauri/src/lib.rs` calls the same first-GPU detection function.
- Generation uses the existing `generate::run_with_control` and emits progress events; normal desktop generation stores into configured output directory and history.
- CLI setup derives a profile from VRAM if none is provided.

| File | Planned responsibility |
|---|---|
| `crates/qil-core/src/gpu.rs` | Enumerate NVIDIA GPUs with UUID, PCI identity, name, total/used VRAM, driver, and supported capability information where reliably obtainable |
| `crates/qil-core/src/config.rs` | Persist GPU selection mode/identity and verification result with serde defaults |
| `crates/qil-core/src/setup.rs` | Shared compatibility checks and setup status/recommendation, enforce 10 GB before downloads |
| `crates/qil-core/src/engine.rs` | Launch ComfyUI with selected GPU bound for CUDA |
| `crates/qil-core/src/generate.rs` | Reuse existing generation path for a controlled verification request if extraction is needed |
| `crates/qil-core/src/gpu_profile.rs` (new, if decomposition is useful) | Pure GPU/profile eligibility and recommendation policy with unit tests |
| `crates/qil-core/src/lib.rs` | Export any new policy module |
| `app/src-tauri/src/lib.rs` | Desktop commands for GPU select, verification, retry/skip, selected-GPU stats |
| `app/ui/index.html` | Setup GPU selector/profile recommendation and verification panel/status |
| `app/ui/app.js` | Setup/verification state transitions and retry/skip actions |
| `app/ui/i18n.js` | EN/RU/ES copy for compatibility, recommendation and verification |
| `app/ui/styles.css` | GPU list, verification progress/result state |
| `app/ui/tests/gpu-setup.test.js` (new) | Frontend state flow tests with mocked Tauri commands |
| `crates/qil-core/src/gpu.rs` unit tests and/or new policy tests | Parsing, eligibility, profile-boundary, selection tests |
| `README.md`, `CHANGELOG.md` | Support floor, setup recommendation and verification behavior |

Before implementation, execution agent must inspect the current complete setup flow and determine how the UI transitions on app startup versus post-install. Avoid assuming the previously discussed prompt-preset branch has already been implemented; preserve any intervening changes.

## 2. Decisions and Explicit Limits

1. **One GPU selection is persisted by stable UUID.** Enumerate every NVIDIA GPU. If one supported device exists, select it automatically. If multiple supported devices exist, preselect the recommended device (initial recommendation: most total VRAM, stable tie-break by UUID) and allow user choice before setup. Do not use display order as persistent identity.
2. **Auto/manual profile choice is separate from GPU choice.** Default is automatic. Manual profile selection is retained and is not silently overridden by later detection. Explain if a manual choice may not fit the selected card.
3. **Eligibility is distinct from transient readiness.** Name/model, architecture compatibility, driver, and total VRAM determine support. Current free VRAM is shown as a readiness hint and may defer verification, but must not permanently reject a qualifying GPU.
4. **Unsupported architecture must not be guessed from marketing name.** First verify a reliable discovery source for CUDA compute capability on supported Windows/driver combinations. Prefer querying `nvidia-smi` UUID/model plus capability via a small bundled-runtime Python/PyTorch probe only after dependencies exist; for the pre-download gate, use an explicit, reviewed supported-device policy if capability cannot be queried reliably. Do not claim pre-download architecture validation without an implementation that actually provides it.
5. **Verification must have bounded runtime and use existing cancellation.** No bespoke indefinite generation path. Use fixed Draft, fixed prompt/seed, a finite timeout, and existing run control.
6. **Do not add VRAM-dependent engine tuning without measured evidence.** This change chooses the existing Balanced/Compact encoder profile, binds the selected GPU, and records observed memory/time. It does not introduce speculative engine arguments.
7. **Do not install extra profiles or models automatically as a fallback without explaining the additional download.** If Balanced verification fails specifically due to OOM, recommend Compact and calculate/show the additional model download before the user accepts switching.

## 3. Task 1 — Enumerate NVIDIA GPUs and Test the Pure Recommendation Policy

**Files:** `crates/qil-core/src/gpu.rs`; optionally new `crates/qil-core/src/gpu_profile.rs`; `crates/qil-core/src/lib.rs` if new module.

### Discovery contract

Introduce a stable serializable GPU identity, extending `Gpu` or adding a separate type:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    pub uuid: String,
    pub pci_bus_id: String,
    pub name: String,
    pub total_mb: u32,
    pub used_mb: u32,
    pub util: u32,
    pub driver: String,
    // Add compute capability only if reliably discoverable and tested.
}

pub fn detect_all() -> Result<Vec<Gpu>, GpuDetectError>;
pub fn detect_selected(uuid: Option<&str>) -> Option<Gpu>;
```

Use an explicit `nvidia-smi --query-gpu=uuid,pci.bus_id,name,memory.total,memory.used,utilization.gpu,driver_version --format=csv,noheader,nounits` query. Parse all rows, tolerate a final newline, reject malformed required fields rather than silently assigning incorrect identity. Preserve the current no-console Windows process behavior. Keep a compatibility wrapper only if current call sites need it; migrate all app/setup call sites to selected identity.

### Recommendation policy

Implement a pure function that returns a structured result (eligible/rejection reason/recommended profile), independent of process invocation:

```rust
pub struct GpuRecommendation {
    pub supported: bool,
    pub profile: Option<Profile>,
    pub reason: Option<GpuSupportReason>,
}

pub fn recommend(gpu: &Gpu, capability: Option<&GpuCapability>) -> GpuRecommendation;
```

Apply these exact VRAM thresholds:

```rust
if total_mb < 10_000 { unsupported }
else if total_mb < 12_000 { Compact }
else { Balanced }
```

Use MB units returned by `nvidia-smi`; document that these thresholds are decimal GB (10,000 MB and 12,000 MB) for consistent product behavior. Ensure a machine with 10,239 MB is Compact, and 12,000 MB is Balanced. The existing `Profile::for_vram` should become this shared policy only if its callers are all intended to use identical thresholds; otherwise move the threshold into a clearly named profile policy and migrate CLI/setup callers deliberately.

### TDD

Write parser and recommendation tests before implementation. Run:

```powershell
cargo test -p qil-core gpu
```

Cover multi-row query, whitespace, malformed row, nonzero process exit, zero GPUs, stable UUID/PCI ID, 9,999/10,000/11,999/12,000 MB boundaries, old driver, incompatible capability, and the difference between total and currently free VRAM. Use fixtures; unit tests must not require a real GPU.

**Acceptance:** All devices are discoverable by stable UUID; eligibility/profile selection is deterministic and fixture-tested.

## 4. Task 2 — Persist GPU Selection and Verification State

**File:** `crates/qil-core/src/config.rs` and associated tests.

Add serde-defaulted config fields without breaking older config files:

```rust
#[serde(default)]
pub gpu_selection: GpuSelection,
#[serde(default)]
pub gpu_verification: Option<GpuVerification>,
```

Proposed serialized structures:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GpuSelection {
    pub uuid: Option<String>,
    pub profile_mode: ProfileMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProfileMode {
    #[default]
    Auto,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GpuVerification {
    pub gpu_uuid: String,
    pub profile: Profile,
    pub engine_fingerprint: String,
    pub status: VerificationStatus,
    pub checked_at: String,
    pub seconds: Option<f64>,
    pub peak_vram_mb: Option<u32>,
    pub message: Option<String>,
}
```

`VerificationStatus` should cover `passed`, `skipped`, and `failed`. Missing fields in existing config deserialize to `uuid: None`, Auto mode, and no verification. Update `Config::default()` and all explicit `Config { ... }` test fixtures. Do not bump config version unless a real migration is required; serde defaults are the migration path.

Verification freshness is valid only for the current GPU UUID, active profile, and engine fingerprint (at minimum pinned ComfyUI commit + relevant model/engine dependency identity). A GPU/profile/fingerprint change invalidates a previously passed result. Do not store raw error logs, just a bounded user-readable summary.

Tests: old JSON config loads; new state round-trips; Auto/Manual persists; verification invalidates when GPU/profile/fingerprint differs; config save failure does not change in-memory state.

**Acceptance:** Existing installs remain loadable and selected GPU/profile/test status survive app restart.

## 5. Task 3 — Enforce GPU Eligibility and Apply Auto/Manual Profile During Setup

**Files:** `crates/qil-core/src/setup.rs`, profile policy/config from earlier tasks, `app/src-tauri/src/lib.rs`, `app/ui/index.html`, `app/ui/app.js`, `app/ui/i18n.js`, `app/ui/styles.css`.

### Core/setup behavior

- Return all detected GPUs and recommendation in setup status (or add a focused `gpu_inventory` command if keeping `Status` stable is cleaner).
- Before computing model downloads and before `setup::run` begins, reject: no NVIDIA GPU, no selected UUID among detected devices, unsupported capability, driver below `MIN_DRIVER_MAJOR`, total VRAM below 10,000 MB.
- Make desktop `setup_configure` accept selected UUID and profile mode in addition to profile/models directory. Validate selected UUID against fresh discovery; do not trust arbitrary frontend text.
- Auto mode recomputes the profile for the selected GPU before status/download calculation. Manual mode preserves the chosen profile but still enforces GPU/driver/capability and 10 GB floor.
- Add the same 10 GB hard floor and use the shared profile recommendation in CLI setup. Explicit CLI profile selection remains manual and should produce a clear error for unsupported GPU hardware.
- Ensure model list/download calculations follow the chosen effective profile, so Compact downloads the w4a8 encoder and Balanced downloads int8.
- Persist validated config only after all changes pass validation. Preserve existing output/model path behavior.

### Setup UI

- Display every detected device with model and total VRAM. Show recommendation/rejection reason with translated text.
- Auto-select the sole supported GPU. With multiple supported GPUs, default to largest VRAM then stable UUID tie-break and let user choose before install.
- If no supported GPU exists, disable Install and display actionable reason before model downloads.
- Show an Auto/Manual profile control. Auto displays the recommendation explanation; changing profile switches to Manual. Provide an explicit return-to-Auto action.
- Show current free VRAM as a non-blocking hint only. A busy GPU does not make hardware unsupported.
- On language change, rerender labels without losing selected UUID/profile mode.

### TDD and verification

Add pure setup/config tests for no GPU, multiple GPU selection, absent saved UUID, under-10 GB, driver floor, incompatible compute capability, automatic thresholds, manual retention, free-memory pressure, and correct model step list. UI tests mock inventory/config commands and verify install remains disabled for an unsupported card.

Run:

```powershell
cargo test -p qil-core
cargo test -p qil-cli
```

**Acceptance:** Unsupported systems are blocked before model downloads; qualified GPU and selected profile determine exact download steps; CLI and desktop use one policy.

## 6. Task 4 — Bind the Engine and GPU Telemetry to Selected Device

**Files:** `crates/qil-core/src/engine.rs`, `crates/qil-core/src/gpu.rs`, `app/src-tauri/src/lib.rs`, relevant tests.

1. Set the environment for the ComfyUI child process so CUDA sees only the selected physical GPU. Preferred mechanism is `CUDA_VISIBLE_DEVICES=<selected UUID>` if verified for the shipped CUDA/PyTorch stack on Windows; otherwise use the tested equivalent by stable device index/UUID and document its remapping semantics. Do not use a stale ordinal as the persistent selection.
2. Verify ComfyUI/PyTorch sees the intended device using the same runtime interpreter. A mismatch must fail engine start with a clear message, not silently fall back to GPU 0.
3. Update GPU polling and `gpu_stats` to report the selected physical GPU via UUID. Handle selected GPU missing after startup with a clear unavailable state and prompt reselection/restart.
4. Keep external ComfyUI detection behavior understandable: if an external engine is already running and cannot be verified as bound to the selected device, disclose this and do not claim selected-GPU verification.
5. Ensure selection environment is applied only to the spawned engine process, not globally to the app or CLI process.

Test command/environment construction through an injected command factory or pure helper. Add fixtures for UUID-to-device binding and ensure unrelated GPU rows do not change reported selected memory.

**Acceptance:** On a multi-GPU machine, chosen GPU is the one used by ComfyUI and shown by telemetry; missing or mismatched selection is explicit.

## 7. Task 5 — Implement Bounded Verification Generation Backend

**Files:** `app/src-tauri/src/lib.rs`; optionally a focused `crates/qil-core/src/verification.rs` if shared test setup belongs in core; reuse `crates/qil-core/src/generate.rs` and existing `RunControl`.

### Command contract

Add focused commands (names may be refined to match conventions, but keep semantics):

```rust
#[tauri::command]
async fn gpu_verification_start(/* app, engine, settings */) -> Result<(), String>;

#[tauri::command]
async fn gpu_verification_cancel(/* verification state */) -> Result<(), String>;

#[tauri::command]
fn gpu_verification_status(/* settings */) -> Option<GpuVerification>;
```

Use a dedicated atomic/mutex guard so a verification cannot overlap another verification or normal generation. Reuse the app's existing generation cancellation and progress channel if practical; otherwise add a distinct `gpu-verification-progress` event and cancellation sender. Do not weaken BUSY/cancellation invariants in `generate`.

### Test request

Use existing `GenRequest` fields:

```rust
GenRequest {
    mode: "text".into(),
    prompt: "A simple red apple on a plain neutral background.".into(),
    images: vec![],
    preset: "draft".into(),
    aspect: "1:1".into(),
    seed: Some(42),
    transparent: false,
}
```

Bound runtime with a finite timeout (recommend max 3 minutes for the generation after engine ready; engine startup must also have a bounded wait). Progress events distinguish engine startup, model load, and generation stages. Provide cancellation for Skip.

### Isolated output and cleanup

- Use a dedicated path under runtime, e.g. `runtime_dir/verification/run-<id>/`, never configured user `output_dir`.
- Restrict asset protocol only if the UI needs a thumbnail; simplest first version need not display the test image.
- On success, record elapsed seconds and peak VRAM, then delete the verification output and temporary folder.
- On failure/cancel, clean partial image/temp files after engine confirms request cancellation; do not delete files while ComfyUI may still be writing.
- Never add test output to `sessionOutputs` or normal history.

### Measurement and classification

- Poll `nvidia-smi` by selected UUID throughout the test and record peak `memory.used`; do not report utilization from another card.
- Distinguish successful generation, confirmed user cancellation/skip, timeout, CUDA/driver error, and likely OOM. Do not infer OOM solely from a generic timeout.
- Persist status for success/skip/failure, current UUID/profile/fingerprint, timestamp, duration and peak memory where available.
- If active VRAM is temporarily insufficient before starting, keep the GPU eligible and offer “close other GPU applications and retry” rather than marking unsupported.
- If Balanced has a confirmed OOM and Compact is available, recommend switching profile; calculate the additional encoder download from setup status and require confirmation before downloading it. Do not automatically initiate a multi-GB download from the failure handler.
- The app remains usable after skip/failure, with “Not verified” status and retry action in settings/status.

Unit tests use mocked generation/telemetry to cover success, skip, timeout, OOM classification, cleanup, persistence, and no overlap. Real GPU testing is manual because CI has no guaranteed NVIDIA hardware.

**Acceptance:** Verification is bounded/cancellable, produces no user-facing image/history item, records meaningful result, and cannot race normal generation.

## 8. Task 6 — Add Post-Install Verification UX and Retry

**Files:** `app/ui/index.html`, `app/ui/app.js`, `app/ui/i18n.js`, `app/ui/styles.css`, new `app/ui/tests/gpu-setup.test.js`.

1. After setup completes and `setup_status` confirms all components/models ready, show a verification phase before normal app entry.
2. Start verification automatically once per unverified `(GPU UUID, profile, engine fingerprint)` combination. Avoid automatic retry loops on each app launch after a failure/skip.
3. Show the selected GPU/profile, plain-language purpose, stage/progress, and actions **Skip** and, after a result, **Continue**. A skip sends cancellation, waits for confirmed safe completion, then enters app and records status `skipped`.
4. Success screen reports verified baseline, measured duration and peak VRAM, then Continue enters app.
5. Failure screen shows actionable explanation and Retry; offer profile switch only for confirmed memory failure. Do not make verification a permanent gate to image generation.
6. Expose verification status and “Run verification again” from settings or an existing setup/status surface. Do not automatically test again unless GPU/profile/engine fingerprint changes or user asks.
7. Disable conflicting app controls while verification owns generation. Do not reuse normal `generate()` UI if it would add images to history or mutate prompt/reference state.
8. Add translated strings in EN/RU/ES for all GPU discovery/reason/profile/verification phases and errors.
9. Test state transitions with mocked Tauri `invoke` and `listen`: automatic start once, skip waits for cancellation, success Continue, failure Retry, app available after failure, restart does not loop, changed fingerprint prompts again, and ordinary generation controls do not race verification.

**Acceptance:** New install proceeds setup → verification (automatically started; skippable) → app; retry is available later without polluting user output/history.

## 9. Task 7 — CLI Consistency and Documentation

**Files:** `crates/qil-cli/src/main.rs`, `README.md`, `CHANGELOG.md`.

- `qil setup` uses the same supported GPU enumeration, 10 GB floor, selection policy, and 10/12 GB profile boundaries.
- If multiple supported cards exist, CLI must either accept an explicit stable GPU selector or print detected devices and require an unambiguous selection; never silently select a different card than desktop policy.
- Preserve existing `--profile` override but make it explicit/manual and reject unsupported hardware even if user forced the profile.
- `qil doctor` reports all detected GPUs, selected UUID/model/VRAM, support reason, automatic recommendation vs configured profile, verification state/freshness, and free VRAM as transient information.
- Do not add a CLI verification benchmark unless needed for shared implementation; this feature's automatic post-install UX is desktop-first. If the backend verification command is designed as a shared core API, document a later CLI entry point as out of scope for this implementation.
- README documents supported NVIDIA/Windows, 10 GB minimum, profile thresholds, automatic test and Skip, and the meaning/limits of “verified”. Changelog follows existing format.

Run:

```powershell
cargo fmt --all -- --check
cargo test --workspace
```

Do not run the ignored real-download test.

**Acceptance:** CLI setup and doctor no longer contradict desktop hardware policy.

## 10. Task 8 — End-to-End Verification and Release Build

From `app/`:

```powershell
node --test ui/tests/*.test.js
```

From repository root:

```powershell
cargo fmt --all -- --check
cargo test --workspace
./build.ps1 -Installer
```

Manual test matrix (real NVIDIA machine required):

1. No NVIDIA driver/GPU: setup explains unsupported state before model downloads.
2. NVIDIA <10 GB: blocked before model downloads; no attempt to fetch profile models.
3. 10–<12 GB supported card: Compact recommended; selected device remains bound at engine launch.
4. 12 GB card: Balanced recommended.
5. Driver below minimum or unsupported compute capability: explicit actionable reason and blocked setup.
6. Multi-GPU system: choose non-first GPU, verify engine device, name and VRAM meter match selected UUID.
7. Qualifying GPU with VRAM consumed by another process: still eligible; show transient memory hint and allow close-app/retry.
8. Successful Draft test: status persisted, output cleaned, no item in history/user output.
9. Skip during model load and during sampling: cancellation is confirmed before cleanup/app entry; status is skipped.
10. Timeout/OOM: actionable state, no repeated launch loop, user can retry; Compact fallback requires confirmation and displays download estimate.
11. Restart after success: no automatic rerun while verification fingerprint matches.
12. Change selected GPU/profile or engine/model fingerprint: verification is stale and offered again.
13. Existing old config with no new fields: app opens and populates valid selection/recommendation without losing settings.
14. Language EN/RU/ES and light/dark appearance.

Capture setup, verification-success, verification-failed/skip, and selected-GPU status screenshots. Final handoff records test outputs, hardware used for real verification, build artifact paths, and any GPU matrix combinations not physically available for validation.

## 11. Completion Criteria

- All NVIDIA devices are enumerated with stable identities and supported devices are selected deterministically.
- Hardware floor, driver, and capability checks occur before model downloads.
- Auto profile thresholds are Compact at 10–<12 GB and Balanced at >=12 GB; manual selection is preserved.
- Engine and telemetry target the same selected GPU.
- Existing config loads with defaults; selection and verification status persist safely.
- Post-install Draft verification starts automatically once per current configuration, is bounded and skippable, and is retryable later.
- Verification artifacts never appear in user output/history.
- Failure does not permanently block using the app; transient VRAM pressure is not mislabeled as unsupported hardware.
- Workspace/UI tests pass, installer builds, and real-GPU QA evidence is reported honestly.
