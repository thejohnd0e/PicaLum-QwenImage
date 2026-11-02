# Compact Quality Controls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Compact the quality section, add real manual generation controls, remove negative prompt from desktop UI, and retain an investigation backlog item.

**Architecture:** Core resolves preset/override parameters and validates them before engine I/O. A small desktop controller manages the collapsible presets, editable values and Custom state; app.js snapshots resolved options for a complete batch. CLI/core negative support stays intact.

**Tech Stack:** Rust/Serde, Tauri 2, plain HTML/CSS/JavaScript, Node test runner.

**Spec:** `docs/superpowers/specs/2026-11-02-quality-controls-design.md`.

## Global Constraints

- Minimum is 256 pixels per axis; dimensions increment by 32 pixels. This is an application policy, not a verified model minimum.
- Format owns the nominal aspect ratio; display actual rounded dimensions.
- Preserve existing presets and request behavior when overrides are absent.
- Remove desktop negative persistence/requests, not CLI/core negative functionality.
- Keep CFG, denoising, masked-edit source dimensions, comparison and history intact.
- No new dependencies, GPU tests, model downloads or negative-prompt experiments without separate permission.
- Do not touch `.agents/` or `skills-lock.json`; stage only scoped files.
- Increment version by exactly one before the release build; at planning time 0.1.24 → 0.1.25, subject to fresh version inspection.

## Review Focus

1. Previously saved negative text must never reach a desktop request after removing its field (Task 1).
2. Invalid typed numbers, stale localStorage and rounding at 256 must not submit a different or invalid size (Tasks 2–3).
3. Preset → Custom → preset and aspect changes must display and submit the same resolved values (Tasks 2–4).
4. Batch snapshots and Surprise/Random must not leak current UI changes or freeze all random images to one aspect (Task 4).
5. Photo and masked-edit controls must not promise ignored canvas overrides or change source-sized compositing (Tasks 2–4).

## Preflight / approval gate

- [x] Read `AGENTS.md`, the spec, `git status --short`, and current manifests. Preserve unfamiliar work.
- [x] User confirmed: Steps editable in all modes; photo Width/Height source-derived/read-only; manual bounds are 1–100 steps, 256–4096 per axis, 4,500,000 pixel area.
- [x] Use an isolated worktree. Do not run concurrent builds against the same target directory.

## File map

- `crates/qil-core/src/generate.rs`: backward-compatible overrides, preset resolution, validation, graph execution and actual result metadata.
- `app/src-tauri/src/lib.rs`: parameter-resolution command and registration; masked-edit steps forwarding.
- `crates/qil-core/src/masked_run.rs`, `crates/qil-cli/src/main.rs`, `crates/qil-core/src/verification.rs`: update Rust constructors; optional masked steps and metadata. No new CLI flags are required.
- `app/ui/quality-controls.js` (new): small independently testable quality state/controller, no copied preset table.
- `app/ui/index.html`, `app/ui/styles.css`, `app/ui/i18n.js`, `app/ui/app.js`: controls, integration, translations, persistence and snapshots.
- `app/ui/masked-edit-run.js`: snapshot/forward optional steps from the quality state into masked editing.
- `app/ui/tests/quality-controls.test.js` (new), existing `prompt-adherence.test.js`, `edit-controls.test.js`, `masked-edit-run.test.js`: regression coverage.
- `docs/negative-prompt-investigation.md` (new): deferred investigation, not a claim of model incompatibility.
- `CHANGELOG.md`, root Cargo manifests/lock, app npm manifests/lock and Tauri config: one release version increment.

## Task 1: Remove the desktop negative field and record the investigation

**Interfaces:** Desktop generation omits `negative`; Serde's existing default supplies an empty string. CLI `--negative` and `GenRequest.negative` are unchanged.

- [x] Update `app/ui/tests/prompt-adherence.test.js`: assert shipped HTML has no negative field and desktop code no longer reads legacy storage or adds negative to snapshots/requests. Preserve CFG assertions.
- [x] Run `node --test app/ui/tests/prompt-adherence.test.js`; confirmed expected failures against current negative field/wiring.
- [x] Remove field/label, state initialization, assignment, listener, busy selector, snapshot/request values and unused label/placeholder strings in all three languages. Existing storage key is left unread; CLI/core are unchanged.
- [x] Create `docs/negative-prompt-investigation.md` with deferred checklist, previous A/B limitations, source links and known masked-edit behavior.
- [x] Run `node --test app/ui/tests/*.test.js` (87/87 pass) and `cargo test --workspace` baseline (151 passed, one ignored real-download test); commit scoped UI/test/document files.

## Task 2: Resolve and validate manual settings in core

**Interfaces:** Add `#[serde(default)] steps: Option<u32>`, `width: Option<u32>`, `height: Option<u32>` to `GenRequest` (camelCase unchanged). Add serializable `ResolvedGenerationSettings { steps: u32, width: u32, height: u32 }`. Define `resolve_generation_settings(req: &GenRequest) -> Result<ResolvedGenerationSettings, String>` for text/Surprise canvas settings. It consumes the existing `preset()` and `dims()` and confirmed bounds; never writes a `custom` preset string. Width/height are an atomic optional pair. Photo resolution remains source-derived.

- [x] Add Rust tests: legacy requests, Draft and all seven Max dimensions, exact 256 custom size, invalid steps/dimensions/area, no aspect recomputation, photo steps and photo dimension rejection.
- [x] Run `cargo test -p qil-core --lib`; first compile failed because resolver API was absent; after implementation focused tests pass.
- [x] Implement resolver/validation before uploads, use effective steps in graph/results/JSON/PNG metadata and manual T2I dimensions in latent; preserve photo path and report source-derived canvas dimensions.
- [x] Add optional steps to masked request/Tauri args; forward through core graph and use actual resolved step count in masked output metadata. Update CLI/verification constructors; composite/feather behavior unchanged.
- [x] Expose and register read-only `generation_settings_resolve`; add Tauri test proving shared core resolution.
- [x] Run `cargo test --workspace` (145 core + 5 app + 5 CLI passed, one ignored real-download test); commit core/backend and constructor changes.

## Task 3: Build the compact quality controller

**Interfaces:** `QilQualityControls.create({ getElement, invoke, translate, onChange })` returns `setContext({ mode, aspect, sourcePath }), selectPreset(name), snapshot(), setBusy(busy), render()`. `snapshot()` returns a copy of `{ preset, steps, width, height, custom, pixelBudget }`; photo width/height overrides are omitted, not submitted read-only values. Async resolution must ignore stale responses.

- [x] Write controller/HTML tests covering collapsed disclosure, presets, Custom state, valid/invalid boundaries, aspect recalculation, photo dimensions, random Surprise and storage/stale responses.
- [x] Run the targeted test first; confirmed failure because the controller did not exist, then implement.
- [x] Implement native disclosure, compact summary, labeled bounded inputs, validation and non-blocking sub-1024 detail warning; corrupt saved state falls back to a valid preset.
- [x] Keep Format below; preserve area on ratio changes and query core for source-derived photo dimensions and special preset dimensions. Avoid core calls per keystroke.
- [x] Add English/Russian/Spanish strings for controls, Custom, errors, warning and photo/Random explanations.
- [ ] Actual interactive UI at narrow width/200% zoom and keyboard smoke remain unverified; no generation run.
- [ ] Commit controller/markup/styles/translations/tests with the integrated UI commit.

## Task 4: Wire real generation, batches, Surprise and area editing

**Interfaces:** app.js obtains the Task 3 snapshot once before the batch. Requests include optional `steps,width,height` accepted by Task 2; masked-edit requests include optional steps only under the recommended scope. Resolved return values are authoritative in history metadata.

- [x] Add coverage for the controller's custom/invalid state, fixed/random Surprise, photo request dimension omission, batch snapshots and masked-step wiring/history metadata.
- [x] Instantiate and connect the controller; snapshot once per batch, call `setContext` on mode/Format/reference changes, and pass optional steps to masked edit. Use actual returned step/dimensions in history. Preserve seed/CFG/denoising behavior and CLI defaults.
- [x] Run all Node tests (97 passed) and `cargo test --workspace` (155 passed, one ignored network test).
- [ ] Interactive runtime IPC/UI smoke remains unverified; no GPU generation run.
- [ ] Commit integration and tests with the UI/version changes.

## Task 5: Verify, increment once, build and hand off

- [ ] Review the diff against the spec and resolved decisions; check no ignored active control, duplicate preset table, stale DOM reference, accidental API removal or user-file change.
- [ ] Confirm current version afresh. Increment by exactly one in `Cargo.toml`, workspace package entries in `Cargo.lock`, `app/package.json`, both app entries in `app/package-lock.json`, `app/src-tauri/tauri.conf.json`. Add scoped changelog entry. Planning baseline next version is 0.1.25; do not reuse it if another build has intervened.
- [ ] Run `node --test app/ui/tests/*.test.js`, `cargo test --workspace`, `cargo fmt --all -- --check` and `git diff --check`. Report any failure by name; do not enable ignored real-download tests.
- [ ] Run `./build.ps1 -Installer` once final tests are green. Check `out/qil.exe --version`, desktop ProductVersion and NSIS installer ProductVersion all match the new version.
- [ ] Smoke-test actual desktop startup with historical storage, preset/disclosure state, custom fields, Format changes, Edit read-only dimensions, area-dialog opening, language changes and history. No GPU generation is authorized by this plan alone; use deterministic graph/IPC tests, or request separate authorization for a runtime smoke generation.
- [ ] Commit only scoped changes and report commit, test counts, build version, installer path, open limitations and deferred investigation path. Never stage `.agents/` or `skills-lock.json`.

## Deferred investigation — retain in `docs/negative-prompt-investigation.md`

This is backlog, not a release blocker or a claim that Qwen-Image-2.1 lacks negative support.

- [ ] Record the user's exact positive/negative text, mode, model/checkpoint, quantization, seed, steps and CFG where possible.
- [ ] Verify UI-originated requests independently of CLI; trace text → Tauri → core → actual ComfyUI graph. Note that masked edits historically force negative empty; distinguish that known omission from text/photo behavior.
- [ ] Read current official Qwen, Diffusers and ComfyUI sources for **2.1**, separating original/Turbo, cloud/local and CFG default from actual capability. Source starting points: https://github.com/QwenLM/Qwen-Image-2.1 ; https://huggingface.co/docs/diffusers/api/pipelines/qwenimage21 ; https://docs.comfy.org/tutorials/image/qwen/qwen-image-2-1 .
- [ ] Prior local A/B: CFG 3.0, seed 424242, Draft, positive explicitly requested red cube/blue sphere; negatives changed pixels but failed to remove objects. It was a conflicting-prompt, single-seed test, not proof of unsupported functionality. Historical artifacts: `C:\Users\la\AppData\Local\Temp\opencode\qwen-negative-20261102-020445` (temporary, may no longer exist).
- [ ] With separate GPU authorization, compare neutral/non-conflicting prompts, empty vs targeted negatives, fixed seeds/settings across several cases. Distinguish any pixel change from intended semantic suppression. Include repeats to check reproducibility.
- [ ] If feasible, compare reference Diffusers to ComfyUI with compatible same weights/precision/scheduler. Document unavoidable differences; do not download/load huge alternate checkpoints without authorization.
- [ ] Publish evidence and a bounded conclusion: UI wiring defect, backend implementation difference, quantized-checkpoint effect, model behavior, or still inconclusive. Propose restoring the UI only if there is a demonstrable useful behavior.
