# Responsive Generation Cancellation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make each generation cancellable and bounded across upload, engine queueing, WebSocket/history waiting, result download, and output save, without disturbing unrelated ComfyUI work.

**Architecture:** Preserve the existing `generate::run` API as a CLI-compatible wrapper and add a controlled core API receiving a watch-channel cancellation signal and absolute deadline. The core owns its ComfyUI prompt ID and performs targeted queued-prompt deletion or active-prompt interruption. Tauri registers a sender for the single active run; the UI's Cancel now command signals it, while all completion paths release busy state exactly once.

**Tech Stack:** Rust, Tokio watch channels/select/deadlines, reqwest, tokio-tungstenite, existing Tauri commands, Node built-in syntax checks.

**Spec:** `docs/superpowers/specs/2026-10-09-darkroom-review-batches-and-cancellation-design.md`

## Global Constraints

- Request deadline: 30 minutes; HTTP connect timeout: 20 seconds; HTTP request timeout: 120 seconds; cancellation grace: 10 seconds; WebSocket idle threshold: 30 seconds.
- A finite batch supplies its remaining total budget; a request uses the earlier of the 30-minute request deadline and the supplied remaining budget.
- Cancellation uses only the matching prompt ID. Never clear the complete ComfyUI queue or issue an unscoped interrupt.
- On lost WebSocket events after submission, poll `/history/{prompt_id}` until terminal state or deadline.
- Write final PNGs through a temporary sibling file and rename only after the PNG is complete.
- Keep English as default with existing optional Russian and Spanish.

---

### Task 1: Run Control and Deadline Primitives

**Files:**
- Create: `crates/qil-core/src/run_control.rs`
- Modify: `crates/qil-core/src/lib.rs` (export `run_control`)
- Test: matching `qil-core` unit tests

**Interfaces:**
- Add `RunControl { cancel: tokio::sync::watch::Receiver<bool>, deadline: tokio::time::Instant }` and `RunExit::{Cancelled, TimedOut}` in `run_control.rs`.
- Add `run_with_control(cfg, req, out, control, on_progress) -> Result<GenResult, String>`.
- Keep `run(cfg, req, out, on_progress)` as a compatibility wrapper that creates and holds an uncancelled watch sender and applies the standard 30-minute deadline; existing CLI and benchmark call sites continue to compile unchanged.
- Add private `check_control(&mut RunControl) -> Result<(), RunExit>` where `RunExit` distinguishes `Cancelled` from `TimedOut`.

- [ ] **Step 1: Add failing tests** for not-cancelled, already-cancelled, cancellation while awaiting a delay, before-deadline, and deadline-expired cases.
- [ ] **Step 2: Run `cargo test -p qil-core run_control`** and confirm the missing control behavior fails to compile or fails assertions.
- [ ] **Step 3: Implement the watch receiver/deadline helper** with cancellation-aware `tokio::select!`; retain the watch sender in the wrapper's stack frame until the run finishes.
- [ ] **Step 4: Run `cargo test -p qil-core run_control` and `cargo test -p qil-core`.**

### Task 2: Bounded HTTP, WebSocket, and History Execution

- **Files:**
- Modify: `crates/qil-core/src/generate.rs`
- Modify: root `Cargo.toml` (enable Tokio `net` only if the local HTTP/WebSocket test fixture needs it)
- Modify: `crates/qil-core/Cargo.toml` (enable test-only network support only if needed by the local protocol fixture)

**Interfaces:**
- Every async operation in `run_with_control` receives the same mutable `RunControl` and checks the same absolute deadline.
- Generate a UUID-formatted prompt ID before `POST /prompt`; send it in the prompt body and use it for progress filtering, history lookup, and cancellation even if the submission response is lost.
- HTTP client uses 20-second connect timeout and 120-second request timeout.

- [ ] **Step 1: Add tests around extracted pure helpers** for prompt-ID formatting and timeout-vs-cancel outcome precedence, including a test that a user cancellation is not reported as a timeout.
- [ ] **Step 2: Run `cargo test -p qil-core prompt_id`** and verify the new assertions fail first.
- [ ] **Step 3: Implement prompt-ID creation, explicit client timeouts, and cancellation-aware selects** around reference upload, WebSocket connect, prompt submission, WebSocket receive, history requests/sleeps, result download, and final save.
- [ ] **Step 4: Extract terminal prompt waiting** so WebSocket events and `/history/{prompt_id}` share one terminal-state result (`success`, `engine error`, `cancelled`, `timed out`). Poll history every 750 ms after 30 seconds without WebSocket activity; bound every poll and make it cancellation-aware.
- [ ] **Step 5: Add local protocol tests** using an in-process loopback HTTP/WebSocket fixture to simulate a closed socket after a job is accepted, delayed history, terminal success, terminal error, and never-completing history. Assert that fallback returns the terminal outcome or deadline error and never hangs.
- [ ] **Step 6: Run `cargo test -p qil-core` and `cargo clippy -p qil-core -- -D warnings`.**

### Task 3: Targeted ComfyUI Cancellation and Atomic Save

**Files:**
- Modify: `crates/qil-core/src/generate.rs`
- Modify: `crates/qil-core/src/generate.rs` unit tests

**Interfaces:**
- Cancellation before prompt submission performs no engine interruption.
- Cancellation after prompt ID creation requests removal of only that ID from the queue and requests interruption of only that ID if running; use the exact paths/payloads supported by pinned ComfyUI commit `3b4c0b0e457cf0a51cf3038e0a6750d8f96ce251`.
- If the pinned API lacks targeted queued deletion, fail safely with a clear cancellation limitation; never fall back to global queue clearing or global interrupt.
- Final PNG is written to a unique temporary sibling path, flushed, then renamed to its collision-safe destination; temp output is removed on error or cancellation.

- [ ] **Step 1: Add tests** that request-scoped cancellation sends a prompt ID, no cancellation request is sent before submission, unrelated IDs are never passed, and interrupted save leaves no final partial PNG.
- [ ] **Step 2: Run `cargo test -p qil-core targeted_cancel` and `cargo test -p qil-core atomic_png`** and verify failures precede the implementation.
- [ ] **Step 3: Implement bounded request-scoped queue deletion/interrupt** using the generated prompt ID and a maximum 10-second cancellation grace period; classify response as Cancelled only for user-initiated cancellation.
- [ ] **Step 4: Implement temp-file write, flush, post-write cancellation check, atomic rename, and temp cleanup**. Keep JSON sidecar behavior best-effort after successful PNG publish.
- [ ] **Step 5: Run `cargo test -p qil-core` and `cargo clippy -p qil-core -- -D warnings`.**

### Task 4: Tauri Active-Run Registration

**Files:**
- Modify: `app/src-tauri/src/lib.rs`
- Test: extracted pure helpers in `app/src-tauri/src/lib.rs` or a focused module if the current Tauri test boundary requires one

**Interfaces:**
- Tauri-managed generation control stores `Option<watch::Sender<bool>>` for the active operation.
- Define `GenerationControl { active: std::sync::Mutex<Option<watch::Sender<bool>>> }` and register it with the Tauri builder using `.manage(...)`.
- `generate` registers the sender before awaiting core execution, calls `run_with_control`, and clears the active sender and BUSY flag through structured cleanup on every return path.
- `cancel` signals the active sender and returns promptly; it no longer clears the entire ComfyUI queue or calls an unscoped `generate::interrupt`.
- Extend `generate` with optional `remaining_ms: Option<u64>` for finite-batch deadline propagation; `None` means the standard request deadline.

- [ ] **Step 1: Add a test for active control lifecycle** that verifies one active run is registered and removed on success, cancel, error, and timeout using extracted state helpers.
- [ ] **Step 2: Run `cargo test -p picalum-qwenimage active_generation`** and verify the lifecycle assertion fails before implementation.
- [ ] **Step 3: Implement managed active-run control, remaining-deadline conversion, and prompt-scoped core invocation.**
- [ ] **Step 4: Run `cargo test -p picalum-qwenimage` and `cargo clippy -p picalum-qwenimage -- -D warnings`.**

### Task 5: Cancellation and Timeout UI States

**Files:**
- Modify: `app/ui/app.js`
- Modify: `app/ui/index.html`
- Modify: `app/ui/styles.css`
- Modify: `app/ui/i18n.js`

**Interfaces:**
- **Cancel now** increments the run ID, invokes `cancel`, blocks later batch iterations, and keeps progress visible until the active invoke resolves or the backend reports the bounded cancellation grace outcome.
- **Stop after this image** belongs to finite batches and does not invoke `cancel`.
- A timeout message is distinct from the existing cancelled notice and includes the engine log path when supplied.
- Progress events from stale run IDs are ignored.

- [ ] **Step 1: Add strings in en/ru/es** for cancel-now, stop-after-current, timed-out, engine-may-still-finish, and the log-path hint.
- [ ] **Step 2: Add named controls and separate UI states** without changing the accepted Darkroom layout or the one-click Generate behavior.
- [ ] **Step 3: Wire timeout and cancellation outcomes** so `finally` always clears busy controls and the next run receives a new progress generation ID.
- [ ] **Step 4: Run `node --check app/ui/app.js` and `node --check app/ui/i18n.js`.**
- [ ] **Step 5: Manually verify** cancellation during reference upload, model wait, sampling, history fallback, result download, and save; also verify a lost socket falls back to history and unrelated queued prompts are not removed.

## Final Verification

- Run `cargo test --workspace`.
- Run `cargo clippy -p qil-core -p qil-cli -p picalum-qwenimage -- -D warnings`.
- Run `node --check app/ui/app.js` and `node --check app/ui/i18n.js`.
- Build the desktop app with the repository's documented Windows build command and manually exercise ordinary Generate, cancel-now, request timeout, attached external ComfyUI, and Darkroom progress states.
