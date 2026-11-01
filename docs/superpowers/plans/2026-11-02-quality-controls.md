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

- [ ] Add Rust tests: legacy JSON resolves Standard 1:1 to current `dims("1:1", 1.6)` and 30 steps; Draft and all seven Max dimensions match existing calculations; `steps=17,width=256,height=256` resolves exactly; one missing axis, <256, >4096, non-multiple-of-32, steps outside 1–100, and area >4,500,000 reject; explicit pixel size is not recomputed from `aspect`; legacy no-override graphs remain unchanged. Add photo tests: steps override applies, width/height overrides reject with a clear unsupported-mode error rather than silently ignoring them.
- [ ] Run `cargo test -p qil-core --lib`; confirm the new assertions fail for missing override/resolution support.
- [ ] Implement the resolver and invoke validation before upload, WebSocket or prompt submission. Use effective steps in graph, `GenResult.steps`, sidecar JSON and embedded PNG metadata; audit every use of `p.steps`, including timeout/progress calculations. Use explicit text dimensions in the latent. Keep the no-override photo encoder/init-latent path unchanged.
- [ ] Add `steps: Option<u32>` to `MaskedEditRequest` and optional Serde-defaulted steps to Tauri masked-edit arguments; forward it in `build_gen_request`. Set `None` in existing Rust constructors (CLI, verification, benches/tests as found by compiler). Use effective steps in masked result metadata; leave its final source-sized composite and feather behavior unchanged.
- [ ] Expose `resolve_generation_settings` through a read-only Tauri command `generation_settings_resolve(request: GenRequest) -> Result<ResolvedGenerationSettings, String>` using the same resolver; register it with the invoke handler. This command must not start the engine. Photo UI uses source dimensions plus existing core `edit_canvas_size`, exposed via this command or a separate narrowly typed read-only command; do not reproduce that calculation in JS.
- [ ] Run `cargo test --workspace` (ignored network test stays ignored), confirm override/legacy/masked metadata tests pass. Commit core/backend and constructor changes.

## Task 3: Build the compact quality controller

**Interfaces:** `QilQualityControls.create({ getElement, invoke, translate, onChange })` returns `setContext({ mode, aspect, sourcePath }), selectPreset(name), snapshot(), setBusy(busy), render()`. `snapshot()` returns a copy of `{ preset, steps, width, height, custom, pixelBudget }`; photo width/height overrides are omitted, not submitted read-only values. Async resolution must ignore stale responses.

- [ ] Write `quality-controls.test.js` against the actual controller and HTML. Test collapsed initial state; selected preset summary; preset fills controls; manual edit → Custom; selecting a preset clears Custom; only one selected radio or none in Custom; 256 per-axis boundary; empty/fractional/nonfinite/257 input blocks generation; width/height synchronization with 3:4; invalid companion <256 retains last valid pair and shows an error. Verify rounded ratio is labeled by actual dimensions, not claimed exact.
- [ ] Add stale-response tests: rapidly choose Draft/Max and change Format; late responses cannot restore obsolete values. Add persistence tests for corrupt JSON, old preset-only storage and valid custom settings. Add mode tests for photo read-only dimensions and editable steps; no source yet shows unavailable dimensions rather than a fake value.
- [ ] Run `node --test app/ui/tests/quality-controls.test.js`; verify expected failures before implementing.
- [ ] Implement controller and native `<details id="qualityPresets">`/`<summary>` with localized `Quality presets` and compact current-value summary. Add visible labeled Steps/Width/Height below it, numeric controls with approved bounds, width/height step 32, local validation messages and a small warning below 1024. Persist one versioned quality-settings object and separate disclosure state. Corrupt saved state falls back to valid preset values.
- [ ] Keep Format below as requested. Automatic aspect recalculation preserves area; rounding uses nearest 32, ties upward to match Rust's existing positive rounding. Manual dimension edits preserve the entered valid axis; invalid automatic companion reports an error. Preset dimensions come from core, including special Max sizes. Avoid re-querying core on every keystroke; validate committed numeric input and latest context changes.
- [ ] Add English/Russian/Spanish strings for all labels, Custom, errors, low-resolution warning and photo/Random explanations. Use native labels, summary keyboard interaction, visible focus and no fixed-width translated text.
- [ ] Run all Node tests and render at the screenshot's width plus a narrow desktop window/200% zoom, checking no horizontal overflow. Verify collapse/expand and fields using keyboard only. Commit controller/markup/styles/translations/tests.

## Task 4: Wire real generation, batches, Surprise and area editing

**Interfaces:** app.js obtains the Task 3 snapshot once before the batch. Requests include optional `steps,width,height` accepted by Task 2; masked-edit requests include optional steps only under the recommended scope. Resolved return values are authoritative in history metadata.

- [ ] Write integration tests: choose custom 17 steps and 512×512 → backend request exactly matches; invalid numeric input prevents invoke; subsequent UI edits do not alter a running batch snapshot; preset resets remove stale overrides; switching Create/Edit/Surprise does not leak width/height into photo requests. Verify result history records actual returned steps/dimensions, not a stale preset label.
- [ ] Test Surprise fixed Format and Random separately. Random snapshots a pixel-area budget, then computes rounded dimensions from each `surprise_idea.aspect` after the idea arrives; two different idea aspects produce different dimensions at similar area. A manual dimension edit in Random is unavailable until a fixed Format is selected; Steps remains editable.
- [ ] Test masked IPC payload/metadata carries custom steps, still omits negative, and produces a source-sized final composite. Legacy masked requests without steps retain preset defaults. Test busy/verification locks disable manual inputs and preset actions; re-enable after success, cancellation and failure.
- [ ] Run the targeted Node tests and relevant Rust tests; observe expected failures.
- [ ] Instantiate the controller before generation use; call `setContext` on mode, Format and reference changes. Replace old preset click wiring with controller calls. Update `generate()` validation/snapshot/nextRequest and `masked-edit-run.js` with the tested overrides. Use actual `r.steps` for history metadata. Keep seed, CFG and denoising snapshot behavior intact. No new CLI options are required; keep CLI default behavior unchanged.
- [ ] Run `node --test app/ui/tests/*.test.js` and `cargo test --workspace`; commit integration and tests.

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
