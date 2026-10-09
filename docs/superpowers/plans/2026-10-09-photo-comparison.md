# Photo Comparison and Inspection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the accepted dark Darkroom layout and let users inspect a Photos edit against its original with side-by-side and wipe comparisons, fit/100% zoom, and synchronized pan.

**Architecture:** Reshape the existing desktop shell into the accepted Darkroom arrangement, then add comparison to its central viewer. Extract pure comparison/viewport helpers into a small plain-JS module that is usable by the UI and Node's built-in test runner; the DOM remains managed by `app.js`. Read the original from the first `references` entry already stored in the selected history item's metadata.

**Tech Stack:** Plain JavaScript, CSS, HTML, existing Tauri `convertFileSrc`, Node built-in test runner.

**Spec:** `docs/superpowers/specs/2026-10-09-darkroom-review-batches-and-cancellation-design.md`

## Global Constraints

- Keep English as the default, with Russian and Spanish translations in `app/ui/i18n.js`.
- Keep the dark Darkroom layout: image workspace left, tools right.
- Do not automatically select Russian or Spanish from the operating-system locale; use a saved user choice or English.
- Move Create / Edit / Surprise navigation into the top bar and do not duplicate it in the tool panel.
- Side by side is the default comparison mode; Wipe is secondary.
- Preserve image aspect ratios and source pixels; use contain-fit and show dimensions when inputs differ.
- Respect keyboard focus, reduced motion, and screen-reader names.

---

### Task 1: Comparison Eligibility and Viewport Math

**Files:**
- Create: `app/ui/viewer.js`
- Create: `app/ui/tests/viewer.test.js`
- Modify: `app/ui/index.html` (load `viewer.js` before `app.js`)

**Interfaces:**
- Produces `window.QilViewer.canCompare(item)`, `fitScale(sourceWidth, sourceHeight, viewportWidth, viewportHeight)`, and `normalizedPan(point, sourceSize, viewportSize)`.
- `canCompare` returns true only for `meta.mode === "photos"` with a nonempty first reference path.
- Under Node, export the same pure helpers through `module.exports` when available.

- [ ] **Step 1: Write failing unit tests** for text/surprise items, missing reference metadata, valid Photos references, contain-fit scale, and normalized focal-point clamping.
- [ ] **Step 2: Run `node --test app/ui/tests/viewer.test.js`** and verify the new helper imports fail because the module is not implemented.
- [ ] **Step 3: Implement only the pure helpers** in `viewer.js`; do not touch the DOM in this file.
- [ ] **Step 4: Run `node --test app/ui/tests/viewer.test.js`** and verify all eligibility and geometry cases pass.

### Task 2: Accepted Darkroom Shell and English Default

**Files:**
- Modify: `app/ui/index.html` (move mode navigation into the top bar; keep the tool panel and image stage as app siblings)
- Modify: `app/ui/styles.css` (dark graphite tokens, stage-left/panel-right grid, typography, controls, responsive widths)
- Modify: `app/ui/app.js` (retain mode behavior with top-bar buttons and no duplicate sidebar selector)
- Modify: `app/ui/i18n.js` (saved-language-or-English default and visible `EN` control label)

**Interfaces:**
- Existing `data-mode` buttons and `setMode(mode)` remain the single mode-selection interface.
- Language selection persists via existing `qil.lang`; when absent or invalid, `pick()` returns `en` independent of OS locale.
- Main grid uses image stage first and tool panel second; all existing setup/settings interactions remain functional.

- [ ] **Step 1: Move the existing mode buttons** from the side panel into the top bar and keep their `data-mode` / `aria-selected` behavior; remove the duplicated selector from the panel.
- [ ] **Step 2: Make dark graphite the default CSS palette** and set dark color-scheme controls regardless of OS preference. Use sentence-case labels, generous readable type, quiet borders, and the accepted image-left/tools-right layout.
- [ ] **Step 3: Resize the top bar and panel** to the approved mock proportions, keep the image stage flexible, and add a narrow-window breakpoint that preserves access to every control.
- [ ] **Step 4: Change `pick()` in `i18n.js`** to restore a saved valid language and otherwise return English; display `EN` in the language button and retain Russian/Spanish menu selections.
- [ ] **Step 5: Run `node --check app/ui/app.js` and `node --check app/ui/i18n.js`.**
- [ ] **Step 6: Build and inspect the real desktop app** in English, explicitly selected Russian, and explicitly selected Spanish; verify Create/Edit/Surprise selection, settings modal, setup screen, focus, and narrow-window layout.

### Task 3: Dark Comparison Viewer

**Files:**
- Modify: `app/ui/index.html` (viewer controls, side-by-side panes, wipe layer and divider)
- Modify: `app/ui/styles.css` (dark comparison surfaces, image fit, zoom/pan cursor, divider states, responsive layout)
- Modify: `app/ui/app.js` (`showImage`, `clearCurrentPreview`, comparison mode and reference-load state)
- Modify: `app/ui/i18n.js` (all comparison labels and errors in en/ru/es)

**Interfaces:**
- `showImage(item)` remains the entry point for selecting a history result.
- Comparison sources are `fileSrc(item.meta.references[0])` and `fileSrc(item.path)`; paths never become arbitrary URLs.
- Comparison DOM is hidden unless `QilViewer.canCompare(item)` and the reference loads successfully.

- [ ] **Step 1: Add static comparison markup** with Result / Side by side / Wipe buttons, native dimension labels, Fit / 100% controls, two panes, and a keyboard-focusable wipe divider with `role="slider"` and `aria-valuenow`.
- [ ] **Step 2: Add CSS** for the accepted dark graphite theme, equal contain-fit panes, checkerboard transparency, split layout, wipe clipping, zoom transforms, visible focus, and a narrow-window stack layout.
- [ ] **Step 3: Wire viewer state** to reset to Result on item change, default to Side by side when comparison is entered, and display the first stored reference only for Photos edits.
- [ ] **Step 4: Implement Fit/100%, wheel zoom, drag pan, synchronized normalized focal point, wipe pointer drag, and arrow-key divider changes.** Keep the transform state transient per selected item and provide Fit as reset.
- [ ] **Step 5: Handle missing/unreadable references per pane** without hiding the result or breaking history. When dimensions differ, show each native size and the “fit without cropping” explanation.
- [ ] **Step 6: Add en/ru/es translations** for modes, dimensions, different-size explanation, unavailable reference, zoom, fit, and divider accessible labels.
- [ ] **Step 7: Verify syntax with `node --check app/ui/viewer.js` and `node --check app/ui/app.js`.**
- [ ] **Step 8: Manually verify** Photos with/without reference, Text and Surprise items, different aspect ratios, 100% detail, synchronized pan, wipe drag and arrow keys, Fit reset, image load errors, keyboard focus, and reduced motion in the desktop app.

### Task 4: Collision-Safe Repeated Outputs

**Files:**
- Modify: `crates/qil-core/src/generate.rs` (unique destination selection when the normal timestamp/seed filename already exists)
- Modify: `crates/qil-core/src/generate.rs` tests (destination naming tests)

**Interfaces:**
- Generated output naming preserves the existing timestamp/mode/seed pattern where possible and never overwrites an existing PNG when repeated locked seeds produce the same base name.

- [ ] **Step 1: Add a failing unit test** that requests the same proposed base filename twice and expects two distinct paths while retaining the original name for the first image.
- [ ] **Step 2: Run `cargo test -p qil-core output_filename`** and confirm the test fails before implementation.
- [ ] **Step 3: Add a small pure unique-path helper** that adds a deterministic numeric suffix on collision; use it before writing the image and sidecar.
- [ ] **Step 4: Run `cargo test -p qil-core output_filename` and `cargo test -p qil-core`.**

## Manual QA Gate

- Run the desktop app after the cancellation and batch plans are complete so the final viewer is tested in the accepted Darkroom layout.
- Compare portrait/landscape sources at Fit and 100%; both must remain undistorted and no original file may be written or modified.
