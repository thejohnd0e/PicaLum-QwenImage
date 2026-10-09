# Darkroom Review, Finite Batches, and Responsive Cancellation

## Goal

Refine PicaLum's accepted dark Darkroom interface and improve the three generation workflows requested by the user:

1. Compare an edited photo with its original and inspect details.
2. Generate a bounded number of images without using Generate forever.
3. Cancel safely and ensure a stalled engine or connection cannot leave the UI busy indefinitely.

The UI language remains English by default regardless of the operating-system locale; Russian and Spanish remain available as explicit optional selections and a previously saved language is restored. This work does not change the model, workflow graph, output formats, or existing unlimited Generate forever behavior.

## Decisions

- Keep the accepted Darkroom visual direction: dark graphite workspace, main image area on the left, tools on the right.
- Make the accepted dark appearance the application default rather than switching automatically with the operating-system color scheme. Move the Create / Edit / Surprise mode navigation to the top bar so the right-side tool panel does not repeat the mode selector.
- Keep compact English language selection visible as `EN` in the top bar; Russian and Spanish are selected only through the language menu.
- In Photos mode, comparison defaults to **Side by side**. A **Wipe** control reveals a draggable before/after divider as a secondary mode.
- Fit both images into a common comparison viewport without cropping or resampling. Show each image's native dimensions and explain that the display uses contain-fit when dimensions differ.
- Comparison is available only when the selected result has a usable first reference image. Other modes and items without a reference retain the normal single-image viewer.
- Zoom is shared between both images. **Fit** is the default; **100%** displays one source pixel per CSS pixel. Pan both images together when zoomed. Support wheel zoom, drag pan, and keyboard focus for controls.
- Add bounded batch actions for **4 images** and **8 images**, keeping **Generate forever** as a separate existing action. Show completed/total while running.
- Each request in an unlocked batch gets a fresh random seed, as normal generation does today. When the seed is locked, every request uses exactly that seed; show an explicit warning before starting that the batch will repeat the same seed/composition. Do not silently unlock or alter the seed.
- Output filenames must not overwrite an earlier image when a batch repeats the same seed or when multiple completed outputs share a timestamp; add a numeric collision suffix while retaining the existing filename pattern for the first output.
- **Stop after this image** prevents the next batch request while allowing the in-flight request to finish and be saved. The existing **Cancel now** interrupts the current request. Cancellation is available for ordinary, finite, and forever generation.
- A user cancellation is scoped to the current PicaLum request and must not clear unrelated work from an attached external ComfyUI queue.
- Put explicit time bounds on the end-to-end generation operation and all network calls. Preserve a distinct, user-readable timeout error and release busy state on every exit path.
- When WebSocket events end unexpectedly after queueing, poll `/history/{prompt_id}` until completion, error, cancellation, or the generation deadline. Do not treat a dropped WebSocket alone as proof that the request failed.
- Keep shared execution behavior in `qil-core`; the desktop remains a client of the shared generation implementation.

## Darkroom Viewer and Comparison

The central viewing surface displays one generated image as it does today, with the history strip beneath it. For an edited result with a first reference, expose a comparison control near the image actions:

- **Result**: current single-image view.
- **Side by side**: original reference on the left and generated result on the right. This is the comparison default.
- **Wipe**: two images occupy the same viewport, with a draggable divider and before/after labels.

Comparison state is transient and tied to the selected history item. Selecting a different item resets the mode to Result unless that item is explicitly selected for comparison. Selecting an item without an original reference hides comparison controls and shows the result normally.

For different dimensions, preserve each source's aspect ratio and contain-fit it into an equal-sized viewport. Show native dimensions beside each label and a concise hint such as “Different sizes; both fit without cropping.” In Wipe mode, align both images to the viewport center and disclose that the sources are independently contain-fitted; never stretch one to match the other. The default presentation must not modify source pixels or saved files.

Fit/100% and pan operate in both comparison modes. At Fit, each image is fully visible. At 100%, retain native pixel detail; in Side by side, synchronize zoom and pan around the same normalized focal point, clamping each image independently at its bounds. In Wipe, use the same view transform for both layers. Provide a Fit action to recover from a zoomed/panned state. Respect reduced-motion settings; the wipe divider movement should not animate independently of pointer input.

Reference path is already included in the generated request metadata (`references`). The viewer should use the first reference path for Photos-mode comparison. If it is no longer readable, show an inline unavailable-reference message and keep the result view and other controls usable. Do not treat deleted/renamed reference paths as fatal to history rendering.

## Finite Generation Batches

Extend the existing split Generate menu with **Generate 4 images**, **Generate 8 images**, and the existing **Generate forever**. A regular single Generate remains unchanged. Choosing a finite batch captures one immutable request snapshot exactly as generation currently does, then runs requests sequentially through the existing Tauri generate command; no parallel GPU jobs are introduced.

While a batch is active:

- Show “Image N of M” with the current sampling status.
- Update history and the main preview after every completed image.
- Offer **Stop after this image** and **Cancel now** as distinct actions.
- On normal completion or stop-after-current, report how many images completed; retain all completed files and history entries.
- On an individual generation error, stop the batch, preserve already completed outputs, and present the error normally.
- Prevent edits to controls that affect the captured request, matching the current busy-state behavior.

For a locked seed, show an inline warning in the batch menu and active batch status: “Seed is locked; every image in this batch will use the same seed.” The user can dismiss the menu and unlock the seed. Unlocked requests omit an explicit seed and let `qil-core` create a fresh random seed for each image. Generate forever retains its current seed semantics (the captured locked seed repeats if enabled; otherwise each request gets a fresh random seed).

The batch count and stop-after-current flag are UI orchestration state; they are not new `GenRequest` fields. Each result continues to carry its actual seed and dimensions in existing metadata.

## Cancellation and Bounded Engine Waiting

Refactor generation execution so cancellation and a deadline are observed throughout the complete operation, not just during ComfyUI sampling. Coverage includes reference upload, WebSocket connect, prompt submission, WebSocket receive, history polling, result download, PNG metadata/write, and inter-image delay (if any). Use a single 30-minute absolute deadline for one generation request. For a finite batch, start one absolute batch deadline of `image_count × 30 minutes + (image_count - 1) × 2 minutes`; the extra per-image allowance bounds frontend/Tauri handoff overhead. Pass the remaining batch budget into each request, and enforce the earlier of its 30-minute request deadline or the batch deadline. Generate forever has the per-image deadline but no total-run deadline. Give network connections a 20-second connect timeout and individual HTTP requests a 120-second request timeout. After 30 seconds without WebSocket activity, begin checking history while retaining the existing socket when possible. Report timeouts distinctly; do not add a settings screen for these limits.

The shared `qil-core` generation path receives a cancellation signal and deadline. Before prompt submission, it creates a unique prompt ID and submits that ID with the workflow; this lets cancellation/history lookup remain correlated even if the HTTP response is lost after the server accepted the job. Verify the exact payload and route behavior against the pinned ComfyUI commit during implementation. Async network requests must have explicit connect and request timeouts, and blocking/non-interruptible local file work must check cancellation before and after the operation. A WebSocket receive must not wait forever; when it closes or reaches an idle timeout, move to `/history/{prompt_id}` polling with bounded requests until the prompt reaches a terminal state. All sleep/poll waits must be cancellation-aware.

Tauri tracks the active PicaLum generation request and prompt ID so `cancel` can signal the matching operation. Cancellation before prompt submission exits without interrupting ComfyUI. After submission, use the pinned local ComfyUI API's request-scoped operations: delete a queued prompt by ID and interrupt a running prompt by ID. Never issue a blanket queue clear or global interrupt as part of user cancellation because the app may attach to an external ComfyUI instance. Verify the exact routes and payloads against the pinned commit; the expected local API contract is deletion of only the matching ID through `POST /queue` with `{"delete":[prompt_id]}` and interruption through `POST /interrupt` with `{"prompt_id":prompt_id}`. If the pinned engine does not support targeted queue deletion, do not substitute a global clear; report that the queued request cannot be removed safely. Allow 10 seconds for cancellation to settle; if it cannot be confirmed, return control to the UI, report that the engine may still be finishing, and allow subsequent work to queue normally (ComfyUI remains the serial executor). Do not leave the frontend busy indefinitely.

Frontend state distinguishes:

- **Cancel now**: immediately signals the current request and stops any further batch iterations.
- **Stop after this image**: does not interrupt the active request and suppresses only subsequent iterations.
- **Timed out**: exits busy state, shows an actionable timeout message including the engine log location when relevant, and does not label the result as user-cancelled.

Ensure cancellation/timeout is idempotent and the backend releases BUSY/active request state exactly once through RAII/structured cleanup. Late progress events from a completed or cancelled run must not update the next run's UI.

## Localization and Accessibility

Add English strings as defaults and corresponding Russian and Spanish translations to `app/ui/i18n.js` for comparison modes, zoom controls, image dimensions/alignment notes, batch choices/counts, locked-seed warning, stop-after-current, cancellation, and timeout outcomes. Keep language selection optional; do not change the default language from English.

All new buttons need accessible names and visible keyboard focus. The wipe divider is keyboard-operable with arrow keys, exposes its value to assistive technology, and has a sufficiently large interaction target. Zoom and batch progress must be available to screen readers without announcing every sampling step. Respect reduced motion and preserve existing focus behavior when menus/dialogs close.

## Error Handling

- Missing, unreadable, or dimensionally unusual reference images never break the selected result preview; show an inline comparison-specific explanation.
- Comparison uses safe file URL conversion already used by the viewer and does not accept arbitrary remote URLs.
- If one reference/result fails to load in comparison mode, show a per-pane error and keep mode switching and the result pane available.
- Invalid or disconnected WebSocket falls back to history polling only after a prompt ID exists; before prompt submission, return a bounded connection error.
- On timeout, attempt request-scoped interruption/cleanup best-effort, but preserve the timeout as the reported outcome. If cleanup is unconfirmed after 10 seconds, explain that the engine may still be finishing and subsequent work may queue behind it.
- Cancellation during saving should avoid publishing a partially written final PNG. Write through a temporary sibling file and atomically rename only after the complete PNG is ready; remove the temporary file on error/cancel.
- A stop-after-current action must not mark the current result cancelled; the current result is saved and counted as complete.
- Failure in a later batch member must not roll back previously completed images.
- Repeated-seed batch members receive distinct output paths; saving a later result never silently replaces a previously completed PNG or JSON sidecar.

## Testing

### `qil-core`

- Unit tests for cancellation and deadline helpers, including cancellation during waits and timeout classification.
- Test that WebSocket closure/idle timeout after submission falls back to history polling and returns terminal success/error correctly.
- Ensure every request path is bounded and terminal exits clean up active request state.
- Test cancellation does not invoke a blanket queue clear and passes the active prompt identity to request-scoped cleanup.
- Test output writing leaves no final partial PNG when cancelled before atomic publish.
- Keep existing generation graph and PNG metadata tests passing.

### Tauri backend and UI

- Verify active run registration, cancellation, and release of BUSY state on success, error, cancel, and timeout.
- Exercise finite count 4/8, progress count, stop-after-current, cancel-now, and error after a successful earlier image.
- Verify locked seed repeats exactly and warning is shown; unlocked batch results each use returned generated seeds.
- Test comparison visibility only for Photos results with references; missing references and failed image loads remain recoverable.
- Test Side by side default, Wipe mode, different dimensions/contain behavior, Fit/100%, synchronized pan, keyboard controls, and return to Fit.
- Check English default plus Russian/Spanish strings, keyboard focus, screen-reader labels, and reduced motion.
- Run `node --check` on UI scripts, Rust workspace tests, Clippy, and desktop build. Do not run the network/GPU integration test as part of ordinary CI.

## Scope Exclusions

- No live ComfyUI sampling preview or mask/brush editor.
- No persistent cross-session image library.
- No parallel batch generation, scheduling, or batch resume after app restart.
- No new user-configurable timeout settings.
- No pixel registration/resampling to correct differences between original and generated images; comparison preserves source geometry and explains fit behavior.
- No changes to model weights, setup downloads, CLI batch syntax, or the accepted dark theme beyond the controls and viewer described here.
