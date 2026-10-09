# Finite Generation Batches Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add sequential Generate 4 and Generate 8 actions, batch progress, and Stop after this image while preserving ordinary Generate and Generate forever.

**Architecture:** Keep orchestration in the existing frontend loop, which already serializes Generate forever runs. Extract deterministic batch policy into a plain-JS helper with Node tests. Every image invokes the existing Tauri generate command sequentially and returns its actual seed; cancellation of the active request is supplied by the responsive-cancellation plan.

**Tech Stack:** Plain JavaScript, existing Tauri invoke/events, existing i18n dictionaries, Node built-in test runner.

**Spec:** `docs/superpowers/specs/2026-10-09-darkroom-review-batches-and-cancellation-design.md`

## Global Constraints

- Batch sizes are exactly 4 and 8; do not start parallel GPU requests.
- Existing single Generate and Generate forever retain their behavior.
- Snapshot mode, prompt, references, preset, aspect, transparency, and seed once at batch start.
- An unlocked batch omits explicit seeds so each core request gets a fresh random seed; a locked seed repeats exactly and gets an inline warning.
- Stop after this image allows current generation/save to complete; Cancel now interrupts it and stops the series.
- Preserve successful files/history if a later item fails or the user stops the series.
- Per-image deadline is 30 minutes; finite batch deadline is `image_count × 30 minutes + (image_count - 1) × 2 minutes` and its remaining time is passed to Tauri.
- Keep English default with Russian and Spanish strings.

---

### Task 1: Pure Batch Policy and Tests

**Files:**
- Create: `app/ui/generation-batch.js`
- Create: `app/ui/tests/generation-batch.test.js`
- Modify: `app/ui/index.html` (load helper before `app.js`)

**Interfaces:**
- Export `createBatch(count, nowMs, seedLocked, seed)` returning `{ count, completed: 0, deadlineMs, seedLocked, seed, stopAfterCurrent: false }`; `count` is `4`, `8`, or `null` for forever, and forever has no total deadline.
- Export `shouldStartNext(batch, nowMs)` returning false after user stop, at a finite count, or after the total deadline; otherwise true.
- Export `remainingMs(batch, nowMs)` returning `null` for forever and a nonnegative number for finite batches.
- Export through `window.QilBatch` in the UI and `module.exports` in Node.

- [ ] **Step 1: Write Node tests** for counts 4/8/forever, locked seed preservation, finite deadline formula, zero remaining time, stop-after-current, completed count limit, and unlocked seed as `null`.
- [ ] **Step 2: Run `node --test app/ui/tests/generation-batch.test.js`** and verify the unimplemented module fails.
- [ ] **Step 3: Implement the pure helper** without DOM dependencies.
- [ ] **Step 4: Run `node --test app/ui/tests/generation-batch.test.js`** and confirm all batch policy tests pass.

### Task 2: Batch Menu and Accessible Progress Surface

**Files:**
- Modify: `app/ui/index.html` (`genMenu` and `panel-foot`)
- Modify: `app/ui/styles.css` (menu sizing, warning, progress, stop-after-current control)
- Modify: `app/ui/i18n.js` (batch labels and warnings in en/ru/es)

**Interfaces:**
- Add menu actions with IDs `gen4Btn` and `gen8Btn`; keep existing `genForeverBtn`.
- Add a hidden batch status element with ID `batchStatus` and a distinct button `stopAfterCurrentBtn` shown only during finite batches.
- Leave the current `cancelBtn` as Cancel now for every run type.

- [ ] **Step 1: Add menu items and status markup** with menuitem roles, visible focus styles, `aria-live="polite"` for count changes, and a non-modal locked-seed warning area.
- [ ] **Step 2: Add compact Darkroom-matching CSS** using existing panel variables and reduced-motion rules; do not create extra cards or change the accepted layout.
- [ ] **Step 3: Add all en/ru/es strings** for Generate 4, Generate 8, image N of M, stop after current, locked-seed warning, completed count, and batch timeout.
- [ ] **Step 4: Run `node --check app/ui/i18n.js`** and verify all three language dictionaries retain valid object syntax.

### Task 3: Sequential Batch Orchestration

**Files:**
- Modify: `app/ui/app.js` (`updateGenButton`, `generate`, `cancelGeneration`, progress rendering and event listeners)
- Test: `app/ui/tests/generation-batch.test.js`

**Interfaces:**
- Preserve `generate()` as the single-image entry point and add `generateBatch(count)` for `4`, `8`, or `null` (forever).
- `generateBatch` captures one immutable UI request snapshot, creates `state.batch` with `QilBatch.createBatch`, and calls `invoke("generate", { req, remainingMs })` one item at a time.
- `state.batch.completed` increments only after a successful result is appended to history.
- `Stop after this image` sets `state.batch.stopAfterCurrent = true`; `Cancel now` calls the backend cancellation command and prevents another loop iteration.
- `remainingMs` is sent as `null` for forever and as the live finite budget for each invoke; the cancellation plan adds this Tauri parameter.

- [ ] **Step 1: Add tests for the UI-independent continuation rules**: start next only while below count, no next after stop, no next after deadline, and forever continues until stopped.
- [ ] **Step 2: Run `node --test app/ui/tests/generation-batch.test.js`** and verify the tests fail before wiring the batch state.
- [ ] **Step 3: Refactor the existing generate loop** to accept a batch plan while preserving single and forever defaults. On each iteration, verify run ID and stop state, compute remaining time, request one image, append it, increment count, and update status.
- [ ] **Step 4: Render locked-seed warning** when batch starts with the seed lock checked and keep the snapshot seed unchanged. Unlocked requests pass `seed: null` for every iteration.
- [ ] **Step 5: Implement Stop after this image** separately from Cancel now. On stop, save/count the current image and exit before the next iteration; on cancel/error, do not count the incomplete image.
- [ ] **Step 6: Preserve existing busy-state locking and Generate forever behavior**; disable all three batch menu choices consistently when generation is not available.
- [ ] **Step 7: Run `node --check app/ui/app.js` and `node --test app/ui/tests/generation-batch.test.js`.**
- [ ] **Step 8: Manually verify** single Generate, Generate 4, Generate 8, forever, stop during sampling, cancel during sampling, timeout at the finite deadline, locked/unlocked seed, progress counts, first-success-then-error, and retained outputs.

## Dependencies and Sequencing

- Run the responsive-generation-cancellation plan first so the Tauri `generate` command accepts `remainingMs` and Cancel now is request-scoped.
- The photo-comparison plan is independent; it can be executed before or after this plan.
