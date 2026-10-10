# Mask-Guided Editing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `executing-plans` to implement this plan task-by-task. Complete the prototype/feasibility gate before building the full feature.

**Goal:** Add brush-based masked editing for one source image, using Qwen-Image-2.1 for instructed edits and compositing the generated result so pixels outside the user mask remain unchanged.

**Architecture:** First validate how Qwen-Image-2.1 responds to visual mask guidance using the pinned ComfyUI graph. Then add a mask-aware generation path and a tested image compositor in `qil-core`, with a browser-canvas mask editor in the desktop UI. The model may regenerate the full frame; the application mask limits the published changes.

**Tech Stack:** Rust, `image` crate, Qwen-Image-2.1, ComfyUI pinned runtime, Tauri 2, plain HTML/CSS/JavaScript, Rust and Node.js tests.

**Spec:** Approved in the conversation: add true brush-drawn mask-based editing as a planned feature; current model evidence indicates local instruction editing but no confirmed dedicated mask input. Preserve source pixels outside the selected mask at the application compositing layer. Outpainting and the new reference mode are separate plans.

## Global Constraints

- Do not represent Qwen-Image-2.1 as having a dedicated `mask_image` API; the pinned `TextEncodeQwenImage21` node accepts references, not a separate mask.
- Begin with a prototype and quality gate. Do not ship the full UI if the selected guidance technique produces unacceptable results.
- White mask pixels mean editable; black pixels mean preserve source. Gray values are transition/feathering values.
- Source files are never modified in place.
- Outside the editable mask, final decoded RGBA pixels must be copied from the original exactly.
- The first version keeps result dimensions equal to the EXIF-normalized source dimensions; arbitrary canvas resizing/outpainting are out of scope.
- The final result, not an intermediate model output, is the only output added to the normal history.
- Preserve existing generation cancellation, busy guards, event handling, and metadata behavior.
- Support EN, RU, and ES using current localization conventions.
- Do not commit or publish unless separately requested.

---

## 1. Existing Project Context and File Map

Relevant existing behavior:

- `crates/qil-core/src/generate.rs` builds the Qwen graph, uploads reference images, receives Comfy output, embeds metadata, and publishes the final image.
- With references, `TextEncodeQwenImage21` emits reference conditioning and the target latent. The pinned node has no separate mask input.
- `app/src-tauri/src/lib.rs` already normalizes EXIF orientation for images in `open_upright`; image dependencies are available through `qil-core`'s `image` dependency.
- `app/ui/app.js` owns Edit references, generation state, and history. A masked-edit path must avoid mutating the user's source and must add only the final composite to history.
- `app/ui/viewer-controller.js` contains transform math patterns, but its current role is result comparison. Keep the new mask editor isolated rather than mixing drawing state into it.

| File | Responsibility |
|---|---|
| New `crates/qil-core/src/masked_edit.rs` | Mask validation, optional inward feathering, and deterministic source/result composite |
| `crates/qil-core/src/lib.rs` | Export the new module |
| `crates/qil-core/src/generate.rs` | Add a controlled mask-edit request path or shared output-processing hook |
| `app/src-tauri/src/lib.rs` | Tauri command for mask-edit generation, input validation, temp paths, cleanup, events |
| New `app/ui/mask-editor.js` | Drawing canvas, brush/eraser, coordinate transforms, undo/redo |
| `app/ui/index.html` | Mask editor overlay/dialog and controls |
| `app/ui/styles.css` | Mask editor canvas, toolbar, overlays, responsive layout |
| `app/ui/app.js` | Entry point, request snapshot, result/history integration |
| `app/ui/i18n.js` | EN/RU/ES labels, help text, and errors |
| New `crates/qil-core/tests/masked_edit.rs` or module tests | Compositor and mask behavior tests |
| New `app/ui/tests/mask-editor.test.js` | UI state/coordinate/undo tests |
| `README.md`, `CHANGELOG.md` | Describe mask editing and its actual guarantees/limits |

## 2. Task 1 — Prototype Model Guidance and Decide Feasibility

**Files:** no production changes before passing this gate. Store test inputs/results in an existing ignored or temporary QA location; confirm ignore rules before creating files.

### Questions to answer

1. Does Qwen understand an externally drawn area indicator or a second reference containing the mask?
2. Which representation avoids visible brush marks in the generated output?
3. Does the model make the requested local edit while preserving subject placement and scene context?
4. What amount of mask dilation/feathering is required for plausible boundaries?
5. Is this usable with the pinned ComfyUI commit and shipped quantized profiles?

### Experiment

- [ ] Choose three representative source images: a small object on a textured background, an object near an edge, and a region with a clear boundary.
- [ ] Create binary masks plus a visualization-only marked input. Preserve original, mask, marked image, and raw Comfy output separately.
- [ ] Compare at least these candidate inputs:
  - original as `image_1`, binary/soft mask visualized as `image_2`, with prompt explicitly associating `<image2>` to the editable region;
  - original with a visible circle/paint mark as `image_1`, with prompt explaining the mark;
  - original only with localized natural-language instructions, as a control.
- [ ] Use the same prompt and several fixed seeds per candidate. Record selected profile, steps, elapsed time, and output paths.
- [ ] Inspect whether the mask/mark itself contaminates the result, whether the edit stays local, and whether object boundaries look plausible.
- [ ] Apply a prototype final composite and compare the exact pixels outside the user mask.

### Gate decision

Proceed with the simplest conditioning strategy only if local edit quality is repeatable enough for a beta feature and the mask can be removed/kept out of the final composite. If all candidate guidance is poor, stop before the UI task and report the evidence; propose a separately reviewed model/workflow alternative rather than shipping a misleading brush tool.

**Acceptance:** A documented, reproducible conditioning method is selected, including known limitations and representative before/raw/composite examples.

### Gate result (2026-10-10, RTX 3060 12 GB, `balanced` profile)

Reproduce with `bench/mask_proto/run.ps1` (inputs and masks in `bench/mask_proto/cases.json`; outputs in the ignored `bench/mask_proto/work/`). Three cases (small object on texture, object at the image edge, clear-boundary region), `draft` preset with seeds 101/202/303, then candidate B on `standard` with seeds 101/202.

| Candidate | Result |
|---|---|
| A: mask as `image_2` | Rejected. Seed 101 drew the white mask outline into the output (dog ear); seed 303 recoloured three objects (cafe). |
| B: red outline + tint on `image_1`, prompt explains the mark | **Selected.** 15/15 runs: local edit, mark fully removed in the output, one object edited in the cafe case on every seed. |
| C: instruction only | Works on simple cases, over-edits in the cafe case; compositing trims it but cuts objects at the mask edge. |

Strategy identifier: `marked-reference-v1`. Prompt template (B): `The photo contains a red outline and red tint marking the region to edit. {instruction} Apply the change only inside the marked region. Remove the red outline and tint, and keep everything outside the region exactly the same.`

Known limits and facts for later tasks:
- Compositing is mandatory: the model may still change content outside the mark. Outside-mask pixels were exactly equal to the source in all 33 composites checked.
- The engine returns a different canvas than the source (1248x832 for 1560x1040, 1920x1280 and 1920x1300 inputs), so the raw result must be resampled to the source size before compositing. Aspect ratio can differ slightly (1920x1300 source); resample, do not crop.
- `standard` takes about 125-135 s per edit on this GPU; peak VRAM 11.2 GB of 12 GB.
- Qualification: three cases, two to three seeds; not tested on 8 GB or compact profiles.
- Do not run `qil bench` while the desktop app owns the engine; closing the app mid-run kills the engine and fails the run.

## 3. Task 2 — Implement and Test Mask Compositing in `qil-core`

**Files:** new `crates/qil-core/src/masked_edit.rs`; `crates/qil-core/src/lib.rs`; Rust tests.

### Public contract

Use the project's `image` crate types:

```rust
pub struct MaskedEditOptions {
    /// Feather radius in source-image pixels; 0 means a hard mask.
    pub feather_px: u32,
}

pub fn validate_mask(
    source: &image::RgbaImage,
    mask: &image::GrayImage,
) -> Result<(), MaskedEditError>;

pub fn composite(
    source: &image::RgbaImage,
    generated: &image::RgbaImage,
    mask: &image::GrayImage,
    options: &MaskedEditOptions,
) -> Result<image::RgbaImage, MaskedEditError>;
```

`MaskedEditError` should implement `Display` and `std::error::Error` and distinguish empty mask, dimension mismatch, invalid feather radius, and image-processing failure.

### Semantics

- Source, generated image, and mask dimensions must match before composition. The caller performs any deliberate normalization/resizing; compositor never silently crops.
- A zero mask is rejected before generation at the request-validation layer; the pure compositor may still return a validation error for it.
- Mask value 0 copies the source RGBA pixel exactly; value 255 uses generated RGBA pixel exactly.
- Intermediate mask values blend source/generated with alpha-aware math to avoid dark/bright fringes around transparent pixels.
- Feathering is inward-only: pixels outside the originally selected region remain byte-identical to the source. Feathering must not expand the editable region.
- A fully white mask is valid.
- Work in normalized RGBA image space; metadata/chunk encoding is applied after composition.

### TDD steps

- [ ] Add failing tests for black/white masks, dimension mismatch, transparency, and exact preservation outside mask.
- [ ] Run `cargo test -p qil-core masked_edit` and confirm failures exercise absent behavior.
- [ ] Implement binary composition with exact pixel copying at mask endpoints.
- [ ] Add inward feathering and alpha-aware interpolation.
- [ ] Add tests for feathered edge locality and alpha behavior.
- [ ] Run `cargo test -p qil-core masked_edit`.

Required test cases:

1. All-black mask is rejected as empty.
2. All-white mask returns generated pixel values.
3. Black-mask pixels equal source RGBA exactly.
4. Pixels outside mask remain exactly source after feathering.
5. Mid-gray mask blends expected channel values within rounding tolerance.
6. Transparent source/generated edges do not produce dark halos.
7. Mismatched source/result/mask dimensions return explicit errors.
8. Invalid/oversized feather radius is rejected or clamped by a documented fixed rule.

Export via `pub mod masked_edit;`.

**Acceptance:** Compositor can be unit tested without ComfyUI and guarantees exact source pixels outside the mask.

## 4. Task 3 — Add a Safe Mask-Edit Generation Backend

**Files:** `crates/qil-core/src/generate.rs`, `crates/qil-core/src/masked_edit.rs`, `app/src-tauri/src/lib.rs`.

### Request flow

Add a dedicated Tauri command for mask editing instead of overloading plain text/image generation with implicit behavior. Define and document a request DTO with:

```text
source_path: string
mask_png: bytes or validated temporary path
prompt: string
preset: existing quality preset identifier
aspect/target dimensions: omitted in v1; normalized source canvas is authoritative
seed: optional u64
feather_px: bounded integer
```

Prefer sending mask bytes through the command and writing them to a newly created runtime temp directory. If the existing IPC bridge imposes a practical payload limit, use a Tauri-controlled temp file with a strict app-owned directory and validate the path; never trust an arbitrary frontend path as a privileged output destination.

### Processing order

1. Decode source with EXIF orientation applied, as in `open_upright` behavior.
2. Validate prompt, mask dimensions, mask non-emptiness, and image limits before queueing.
3. Build model input exactly as selected in Task 1; keep source as the composition reference and pass mask guidance using the proven method.
4. Use the existing generation lock, cancel channel, progress events, and finite request deadline.
5. Receive/decode the raw Comfy result; normalize it to the source canvas dimensions using an explicit documented resampling policy only if needed.
6. Composite with `masked_edit::composite`.
7. Embed final metadata and atomically publish the final image into the configured output folder.
8. Clean all temp source/mask/raw-result files after success or confirmed cancellation; never delete a file while ComfyUI may still write it.
9. Return only the final composite to the frontend; add only that path to history.

### Metadata

Record `mode: "masked-edit"`, instruction, seed, actual steps, final dimensions, feather radius, guidance strategy identifier, model/encoder, and generation time. Do not claim the model natively used a binary mask. If mask persistence is not implemented, document that re-running the edit requires recreating the mask.

### Error/cancel behavior

- Cancel before graph submission: remove temporary files and return Cancelled.
- Cancel after submission: wait for existing cancellation acknowledgement/timeout before deleting files Comfy may still read.
- Any failure before publication leaves source and mask intact in UI state and adds no history item.
- File cleanup failures are appended to the user-visible error without hiding the original generation error.
- Never overwrite source path, including when source is inside the output folder.

### Tests

- Unit-test request validation and output path non-aliasing.
- Test cleanup ordering with mocked engine/generation callbacks.
- Test that history-facing result points only to published composite.
- Ensure the global BUSY and generation cancellation guards reject overlapping normal/masked requests.

**Acceptance:** Backend publishes only completed composites, is cancellable, never overwrites the input, and leaves no intermediate in normal history.

## 5. Task 4 — Build the Mask Editor Controller

**Files:** new `app/ui/mask-editor.js`; new `app/ui/tests/mask-editor.test.js`.

Use a focused UMD/IIFE module pattern compatible with `generation-batch.js` and dependency injection for DOM, callbacks, and translation.

### Controller API

```js
const api = Object.freeze({ create });
if (typeof module === "object" && module.exports) module.exports = api;
else window.QilMaskEditor = api;
```

The controller should expose operations equivalent to:

```text
open({ source, width, height })
close()
clear()
undo()
redo()
setBrushSize(px)
setTool("brush" | "eraser")
getMaskPng() -> Promise<Uint8Array or data URL accepted by bridge>
isDirty() -> boolean
setBusy(boolean)
```

### Canvas rules

- Keep mask and drawing coordinates in source-image pixel space.
- Render image and mask overlay into a fit-to-container viewport; translate pointer coordinates through the exact current scale and offset.
- Account for zoom, pan, `devicePixelRatio`, CSS scaling, and centered letterboxing.
- Use Pointer Events and pointer capture; clamp brush strokes to source bounds and stop/clip correctly outside the image.
- Brush writes white; eraser writes black; mask overlay is a display-only translucent color layer.
- Maintain bounded undo/redo history. Store strokes (points, radius, tool) rather than a full-resolution bitmap per action where feasible.
- Changing the source while a dirty mask exists requires an explicit discard confirmation; never silently apply one photo's mask to another.
- Serialize mask as grayscale PNG aligned exactly to normalized source dimensions. Mask has no EXIF orientation of its own.
- Reject submission when no editable pixels exist.

### Tests

Use fake canvas/context or extract pure geometry functions to test:

1. CSS canvas coordinates map to source pixels under fit, zoom, pan, and letterbox.
2. DPR does not change logical mask dimensions.
3. Brush/eraser strokes and undo/redo update expected mask state.
4. Clear and source replacement behavior.
5. Mask serialization uses source width/height and correct white/black polarity.
6. Busy state prevents edits/submission.

**Acceptance:** Brush selection remains aligned with the source at all supported display scales and produces a correctly dimensioned mask.

## 6. Task 5 — Integrate Mask UI Into Edit Mode

**Files:** `app/ui/index.html`, `app/ui/styles.css`, `app/ui/app.js`, `app/ui/i18n.js`, mask editor tests.

### UI

- Add an action such as **Edit an area** for the current first/canvas reference.
- Open a focused editor with image canvas, overlay, Brush/Eraser, brush-size control, Undo, Redo, Clear, feathering control, instruction, Generate, and Cancel/Close.
- Preserve existing reference list and existing non-mask Edit behavior.
- Preview final composite in the existing viewer/history flow after command returns successfully.
- Do not add raw model output or mask overlay to history.
- Display a concise explanation that Qwen follows the instruction and the app applies the mask during final compositing; boundary quality depends on model output and feathering.

### Accessibility and localization

- Give canvas an accessible description and provide keyboard-operable toolbar controls.
- Visible focus, labels, state via `aria-pressed`, and non-color-only tool selection.
- Add EN/RU/ES strings for tools, errors, empty mask, save/run/cancel, and explanatory limitation.
- Escape closes editor only when not generating; during generation it invokes the existing cancel flow.
- Prevent accidental dialog close when there are unsaved mask changes unless user confirms discard.

### App integration

- Snapshot source path, prompt, steps/preset, seed, mask bytes and feather value when Generate starts.
- Disable source/mask/prompt controls while busy; retain mask on generation error for retry.
- Invoke the new command and add the returned final result to session history only once.
- Make ordinary Ctrl+Enter, Escape, and generation-batch behavior consistent with existing Edit flow.

Test frontend state integration with mocked `invoke`/events:

- Masked request contains exact source, prompt, seed/preset and mask payload.
- Successful result enters history once.
- Failed/cancelled result does not enter history and leaves mask editable.
- Repeated submit while busy is blocked.
- Changing original reference with dirty mask prompts before discard.

**Acceptance:** User can select an area in Edit mode, request a change, see the final composite, and retry without losing the mask on failure.

## 7. Task 6 — Visual and Real-GPU QA

### Automated checks

From root:

```powershell
cargo fmt --all -- --check
cargo test --workspace
```

From `app/`:

```powershell
node --test ui/tests/*.test.js
```

Do not run the ignored real-download test.

### Manual checks

- Brush/eraser/undo/redo at default and minimum window sizes.
- Zoomed and panned source, high-DPI display, letterboxed portrait and landscape images.
- Small and large masks, edge-touching masks, hard and feathered edges.
- Object removal, replacement, and color/material edit on real NVIDIA hardware.
- Source with EXIF rotation and transparent PNG.
- Cancel during upload/model work/sampling and verify safe cleanup.
- Compare decoded final pixels outside mask against the normalized source; require exact equality.
- Confirm only final composites appear in history/output and source remains unchanged.
- Check EN/RU/ES, light/dark themes, and keyboard operation.
- Capture before, mask overlay, raw output, final composite, and pixel-difference proof.

Build the app/installer after all automated tests pass:

```powershell
./build.ps1 -Installer
```

## 8. Task 7 — Documentation and Future Work

**Files:** `README.md`, `CHANGELOG.md`, and optionally the existing roadmap document if one exists.

- Describe the feature as **mask-guided editing with application compositing**, not native Qwen inpainting.
- State that source pixels outside the mask are preserved exactly in the final decoded image.
- State that mask interiors may be regenerated beyond the exact brush boundary due to feathering and model/composite behavior.
- Document v1 same-size output limitation and the chosen mask guidance method.
- Add changelog entry only after quality gate and QA pass.
- Keep full Outpainting as a separate future feature.
- Keep reference-only new composition and custom output-size work as separate plans.

## Completion Criteria

1. Prototype passes the documented quality gate.
2. UI produces a source-aligned mask with editable brush/eraser and undo/redo.
3. Qwen conditioning method is accurately described and does not claim a native mask API.
4. Final image preserves decoded source pixels exactly outside the mask.
5. Input is never modified and intermediate outputs are not added to history.
6. Cancellation, failure, cleanup, and retry preserve user work safely.
7. Rust/JS tests and release build pass; real-GPU examples and pixel-comparison evidence are recorded.
