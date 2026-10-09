# Image Output and Continuous Generation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix successful generations that currently fail with `The engine returned no image`, and add a checkbox-controlled mode that generates images sequentially until the user cancels.

**Architecture:** Keep image generation and ComfyUI protocol handling in `qil-core`. Normalize ComfyUI history output in a small pure helper before downloading `/view`. Keep continuous-generation orchestration in the existing plain-JavaScript UI, awaiting each Tauri `generate` invocation before starting the next and using a generation token to prevent cancellation races.

**Tech Stack:** Rust, Tokio, Reqwest, Serde JSON, Tauri 2, plain HTML/CSS/JavaScript, existing ComfyUI HTTP/WebSocket API.

**Spec:** Approved design from the 2026-10-08 conversation; no separate spec file exists.

## Global Constraints

- Preserve one-stage-at-a-time model loading because the model components do not fit together in 12 GB VRAM.
- Preserve engine child-process lifecycle binding so ComfyUI terminates with the parent and VRAM is released.
- Keep uploaded reference filenames unique, including when source photos share a basename.
- Rotate reference images on a copy and never modify the user's original photo.
- Use `npm ci` against `app/package-lock.json`; do not install unconstrained dependencies.
- Keep Qwen model weights treated as non-commercial under the Qwen Research License.
- Do not manually edit `Cargo.lock`.
- Do not add a new frontend framework or JavaScript test dependency.
- Continuous generation is strictly sequential: never invoke a second `generate` command while the first is active.
- A locked seed is reused for every iteration; an unlocked seed is generated independently for every iteration.
- Cancel must prevent a new iteration from starting after the current cancellation request.

---

### Task 1: Add Pure ComfyUI Output Parsing Tests

**Files:**
- Modify: `crates/qil-core/src/generate.rs` in the output-extraction area before `run`
- Test: `crates/qil-core/src/generate.rs` inside the existing `#[cfg(test)]` module

**Interfaces:**
- Produce a pure helper with a concrete signature such as `fn output_image(entry: &Value) -> Result<&Value, String>`.
- The helper must accept both the current ComfyUI shape `outputs.<node>.output.images[0]` and the legacy shape `outputs.<node>.images[0]`.
- The helper must reject missing output metadata and image entries without a non-empty `filename`.

- [ ] **Step 1: Write failing unit tests for the current nested response shape.**

Use a `serde_json::json!` fixture representing a completed history record:

```rust
let entry = json!({
    "outputs": {
        "save": {
            "meta": {"node_id": "save"},
            "output": {
                "images": [{"filename": "ComfyUI_00001_.png", "subfolder": "", "type": "output"}]
            }
        }
    }
});
let image = output_image(&entry).expect("nested SaveImage output");
assert_eq!(image["filename"], "ComfyUI_00001_.png");
```

Add tests for the legacy direct shape, missing `outputs`, empty image arrays, and missing or empty `filename`. Tests must assert an error for invalid structures instead of silently returning an empty `/view` request.

- [ ] **Step 2: Run the focused test and verify it fails.**

Run: `cargo test -p qil-core output_image -- --nocapture`

Expected: compilation or test failure because the helper does not yet exist.

- [ ] **Step 3: Implement the smallest parser helper.**

Iterate over `entry["outputs"].as_object().values()`. For each node, inspect `node["output"]["images"]` first, then `node["images"]`, and return the first image object whose `filename` is a non-empty string. Return a diagnostic error that includes whether the history had no usable image metadata; do not include image bytes or user prompts.

- [ ] **Step 4: Replace the inline extraction in `run`.**

Replace the current `entry["outputs"]...ok_or("The engine returned no image")?` block with the helper. Validate `filename` through the helper before calling `/view`, while preserving `subfolder` and `type` defaults.

- [ ] **Step 5: Run the focused tests and the core test suite.**

Run: `cargo test -p qil-core output_image -- --nocapture`

Expected: all output-parser tests pass.

Run: `cargo test -p qil-core`

Expected: all core tests pass.

- [ ] **Step 6: Commit the isolated parser change.**

Suggested commit: `fix: read ComfyUI image output metadata`

---

### Task 2: Harden `/view` Download and History Diagnostics

**Files:**
- Modify: `crates/qil-core/src/generate.rs` in the `/history` polling and `/view` request path
- Test: `crates/qil-core/src/generate.rs` existing unit tests, only if a pure URL/metadata helper is extracted

**Interfaces:**
- Preserve `generate::run` return type `Result<GenResult, String>`.
- Preserve existing `Progress` events and stage names.

- [ ] **Step 1: Capture the failure boundary in code.**

Keep polling until the history entry is terminal as currently implemented, but distinguish these cases in the returned error: missing prompt history, terminal history with no usable image output, missing filename, and `/view` response failure.

- [ ] **Step 2: Check the `/view` HTTP status before reading bytes.**

After the request, inspect `response.status()`. If it is not successful, return an error containing the status and the image filename. Only then call `.bytes()`.

- [ ] **Step 3: Preserve the existing save behavior.**

Continue creating the destination directory, writing the PNG bytes, writing adjacent JSON metadata, and returning the saved local path. Do not change output directory semantics or metadata fields.

- [ ] **Step 4: Run Rust verification.**

Run: `cargo test -p qil-core -p qil-cli`

Expected: all tests pass.

Run: `cargo clippy -p qil-core -p qil-cli -- -D warnings`

Expected: no warnings.

- [ ] **Step 5: Commit the isolated download/diagnostic change.**

Suggested commit: `fix: report invalid ComfyUI image downloads`

---

### Task 3: Add the Continuous-Generation Checkbox and Translations

**Files:**
- Modify: `app/ui/index.html` near the existing seed controls or generation controls
- Modify: `app/ui/i18n.js` in both `en` and `es` dictionaries
- Modify: `app/ui/styles.css` only if the new control needs existing layout spacing

**Interfaces:**
- Add a checkbox with stable ID `continuous`.
- Add translation key `continuous` for the label and `continuousGenerating` for the active button/status copy if needed.
- Do not add a second generation command or a new backend API.

- [ ] **Step 1: Add the checkbox markup.**

Place it beside the seed controls so it is visible in all generation modes:

```html
<label class="check">
  <input type="checkbox" id="continuous">
  <span data-i18n="continuous"></span>
</label>
```

- [ ] **Step 2: Add English and Spanish strings.**

Use concise translations such as `Continuous generation` and `Generación continua`. If the button label needs to distinguish the mode, add corresponding translated strings in both dictionaries.

- [ ] **Step 3: Run syntax validation.**

Run: `node --check app/ui/i18n.js`

Expected: no syntax errors.

- [ ] **Step 4: Review the layout manually.**

Confirm the checkbox remains usable at the existing desktop width and on the responsive layout. Reuse `.check`; add no new styling unless the current grouping visibly overflows.

- [ ] **Step 5: Commit the UI control separately.**

Suggested commit: `feat: add continuous generation option`

---

### Task 4: Implement a Sequential, Cancellable UI Generation Loop

**Files:**
- Modify: `app/ui/app.js` in `state`, `updateGenButton`, `generate`, and cancel handlers

**Interfaces:**
- Keep the Tauri command contract `invoke("generate", { req })` unchanged.
- Keep the Tauri cancel command `invoke("cancel")` unchanged.
- Add one owner token/counter in UI state, for example `generationRun: 0`, and a boolean such as `stopRequested` only if needed by the implementation.
- Continue storing each successful result as `{ path, meta }` and rendering it through `showImage` and `renderHistory`.

- [ ] **Step 1: Refactor request construction into a single-iteration helper.**

Extract the existing request preparation into a function that receives the fixed mode/preset/aspect/reference snapshot and returns one request. For Surprise mode, generate a fresh idea inside each iteration. For text/photos modes, keep the prompt and references captured when the loop starts.

- [ ] **Step 2: Add a failing cancellation/iteration guard in the existing flow.**

Before each invocation, check the run token and stop flag. After every awaited `invoke("generate")`, check them again before starting another iteration. The loop condition must be equivalent to:

```js
while (continuous && runId === state.generationRun && !state.stopRequested) {
  const result = await invoke("generate", { req: nextRequest() });
  if (state.stopRequested || runId !== state.generationRun) break;
  appendResult(result);
}
```

The exact helper names may differ, but no second `invoke("generate")` may begin until the previous one resolves.

- [ ] **Step 3: Implement per-iteration seed behavior.**

Capture whether the seed checkbox is locked at start. If locked, pass the captured numeric seed for every request. If unlocked, pass `null` for every request so Rust generates a fresh seed for each image. Do not mutate the visible seed input after each iteration.

- [ ] **Step 4: Implement cancellation as an invalidation before the backend request.**

On cancel, increment or replace the active run token and set the stop flag before invoking `cancel`. Disable the cancel button while the backend cancellation request is pending, as the current UI already does. This prevents a completed request from scheduling the next iteration during the cancellation race.

- [ ] **Step 5: Preserve one-shot behavior and error semantics.**

When the checkbox is off, execute exactly one iteration. Treat `Cancelled` as normal termination and show the existing localized cancellation notice. For non-cancellation errors, stop the loop and show the error. Always clear `state.busy`, hide working state, restore buttons, and reset the cancellation flag in `finally`.

- [ ] **Step 6: Append each result immediately.**

After each successful result, build the same history item currently built in `generate`, prepend it to `state.history`, call `showImage(item)`, and call `renderHistory()`. This makes each completed image visible and saved even if a later iteration fails or the user cancels.

- [ ] **Step 7: Update button state for continuous mode.**

While the loop is active, keep the existing Cancel button visible and disable generation inputs that would otherwise change the captured request. The engine stop button is already disabled while busy and must remain so. After completion or cancellation, restore normal controls.

- [ ] **Step 8: Run JavaScript syntax validation.**

Run: `node --check app/ui/app.js`

Expected: no syntax errors.

- [ ] **Step 9: Commit the loop implementation.**

Suggested commit: `feat: generate images continuously until cancelled`

---

### Task 5: End-to-End Verification and Manual Regression Checks

**Files:**
- No new files required

- [ ] **Step 1: Run repository verification commands.**

Run:

```powershell
cargo test -p qil-core -p qil-cli
cargo clippy -p qil-core -p qil-cli -- -D warnings
node --check app/ui/app.js
node --check app/ui/i18n.js
```

Expected: all commands exit successfully.

- [ ] **Step 2: Verify the image-output regression with the installed engine.**

Start the app, generate one text image, and confirm all of the following:

- The generated image appears in the main viewer.
- The PNG exists in the configured output directory.
- The adjacent JSON metadata file exists.
- The image appears in the history strip after restart.
- No `The engine returned no image` notice appears.

- [ ] **Step 3: Verify continuous text generation.**

Enable `Continuous generation`, leave the seed unlocked, start generation, and confirm at least two images complete sequentially. Confirm each image appears in history and has a different seed in its metadata.

- [ ] **Step 4: Verify locked-seed behavior.**

Enable the seed lock and run continuous generation. Confirm each generated JSON metadata file contains the same seed.

- [ ] **Step 5: Verify cancellation during sampling.**

Start continuous generation, press Cancel while the first image is running, and confirm the active request stops, no next request begins, the cancellation notice is shown, and the UI returns to idle state.

- [ ] **Step 6: Verify cancellation between iterations.**

Press Cancel immediately after an image completes. Confirm the loop does not start another image because the run token was invalidated before the next iteration check.

- [ ] **Step 7: Verify Surprise mode.**

Run continuous generation in Surprise mode with a theme. Confirm each iteration produces a new idea and that the loop still stops through Cancel.

- [ ] **Step 8: Record residual limitations.**

The existing `cancel` command interrupts the global ComfyUI queue. This is acceptable while the app owns the only generation request, but should be revisited if multiple independent producers are added later.
