# UI and Prompt Presets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `executing-plans` to implement this plan task-by-task, with verification after each task.

**Goal:** Move the tool panel to the left, replace the app icon, remove the visible Description label, and add editable prompt presets in Edit mode.

**Architecture:** Store prompt presets as validated JSON under the configured runtime directory, with loading and atomic persistence implemented in `qil-core` and exposed through Tauri commands. A focused plain-JavaScript UI module will manage preset selection and editing; the existing generation-quality presets remain unchanged.

**Tech Stack:** Rust, serde/serde_json, tempfile, Tauri 2, plain HTML/CSS/JavaScript, Node.js `node:test`.

**Spec:** Approved design in the conversation: left tool panel; source icon `assets/app-icon.png`; remove the visible Description label while retaining accessible naming; Edit prompt presets save only prompt text, with user-editable names and JSON persistence.

## Global Constraints

- Presets apply to Edit mode and contain only prompt text plus organizational ID/name fields; do not store quality, aspect ratio, seed, or other generation options.
- Preserve all six existing quick-prompt strings exactly, including `<image1>` and `<image2>`.
- Store user data at `<Config.runtime_dir>/presets.json`; default runtime remains `%LOCALAPPDATA%\\picalum-qwenimage`.
- Missing preset file returns built-in defaults without writing a file. A valid user file is authoritative, including an intentionally empty list.
- Invalid JSON or unsupported format must never overwrite the existing file or silently replace user data with defaults.
- Keep the existing desktop UI conventions and support EN, RU, and ES.
- Do not run `crates/qil-core/tests/download_real.rs` with `--ignored`; it downloads model data and is unrelated.
- Do not commit or publish unless separately requested.

---

## 1. Scope and File Map

| File | Responsibility |
|---|---|
| `app/ui/styles.css` | Left/right grid placement and prompt-preset editor styling |
| `app/ui/index.html` | Accessible prompt input, prompt-preset selector/editor, script inclusion |
| `app/ui/app.js` | Integrate prompt-preset controller and remove old Edit quick-button wiring |
| `app/ui/i18n.js` | EN/RU/ES labels and messages for preset controls |
| `app/ui/prompt-presets.js` (new) | Selection/editor controller and UI-side preset operations |
| `app/ui/tests/prompt-presets.test.js` (new) | Node tests for prompt-preset controller behavior |
| `crates/qil-core/src/prompt_presets.rs` (new) | Types, built-in presets, validation and JSON file persistence |
| `crates/qil-core/src/lib.rs` | Export `prompt_presets` module |
| `crates/qil-core/Cargo.toml` | Add `tempfile` dependency for same-directory atomic replacement |
| `Cargo.lock` | Lockfile update for new dependency |
| `app/src-tauri/src/lib.rs` | Tauri commands to load and save presets |
| `app/src-tauri/icons/` | Generated icon assets |
| `README.md`, `CHANGELOG.md` | Document prompt-preset feature and release note |

Existing Tauri icon references are in `app/src-tauri/tauri.conf.json`; they already name `32x32.png`, `128x128.png`, `128x128@2x.png`, and `icon.ico`. Update that config only if icon generation reveals a real path mismatch.

## 2. Preparation in the Execution Session

- [ ] Read root `AGENTS.md` and any additional applicable instructions.
- [ ] Verify the current state of all files listed above; do not overwrite unrelated user work.
- [ ] Confirm `assets/app-icon.png` exists.
- [ ] Check whether the workspace is a Git repository before any Git operation. The environment reported it was not a Git repository when this plan was drafted.
- [ ] Ensure Rust, Node.js, and app dependencies are available. If `app/node_modules` is absent, run `npm ci` from `app/`.
- [ ] If this plan was not committed to disk yet, keep this document at `docs/superpowers/plans/2026-10-09-ui-prompt-presets.md`.

Working directory: `D:\MyProjects\PicaLum QwenImage`.

## 3. Task 1 — Move the Panel Left and Remove the Visible Label

**Files:** `app/ui/styles.css`, `app/ui/index.html`.

1. Set the desktop grid to a fixed-width left panel and flexible right stage:

   ```css
   .app { grid-template-columns: 360px minmax(0, 1fr); }
   .panel {
     grid-column: 1;
     grid-row: 1;
     border-right: 1px solid var(--border);
   }
   .stage { grid-column: 2; grid-row: 1; }
   ```

   Preserve existing declarations on these rules; remove the panel's old `border-left`.

2. Update both responsive grid declarations, which currently put the fixed-width panel second:

   ```css
   /* max-width: 1040px */
   .app { grid-template-columns: 320px minmax(0, 1fr); }

   /* max-width: 680px */
   .app { grid-template-columns: 280px minmax(0, 1fr); }
   ```

3. Remove the visible Description `<label>` from Create's prompt field and give the textarea an accessible name using the existing translation key:

   ```html
   <textarea
     id="promptText"
     rows="7"
     spellcheck="false"
     data-i18n-ph="descPh"
     data-i18n-aria="description"
   ></textarea>
   ```

   Existing `i18n.apply()` maps `data-i18n-aria` to `aria-label`; retain the `description` translation.

4. Manually verify Create, Edit, and Surprise layouts at the default size and minimum window size (1060×680). Confirm the panel scroll and fixed Generate footer still work.

**Acceptance:** Tools are on the left; stage/viewer and history are on the right; no visible Description label; textarea has an accessible name.

## 4. Task 2 — Add the `qil-core` Prompt-Preset Model and Storage

**Files:** new `crates/qil-core/src/prompt_presets.rs`, `crates/qil-core/src/lib.rs`, `crates/qil-core/Cargo.toml`, `Cargo.lock`.

### Data shape

Use a versioned JSON document:

```json
{
  "version": 1,
  "presets": [
    {
      "id": "remove-background",
      "name": "Remove background",
      "nameKey": "qRemoveBg",
      "prompt": "Remove the background, and output a PNG image"
    },
    {
      "id": "user-example",
      "name": "My preset",
      "prompt": "Replace the sky with soft evening clouds. Keep the foreground unchanged."
    }
  ]
}
```

`nameKey` is optional and is reserved for the six built-ins. If a built-in is renamed, the UI clears its `nameKey`; editing only its prompt retains its localized name.

Define and expose these Rust types and functions:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptPreset {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_key: Option<String>,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptPresetFile {
    pub version: u32,
    pub presets: Vec<PromptPreset>,
}

pub fn defaults() -> PromptPresetFile;
pub fn path(runtime_dir: &Path) -> PathBuf;
pub fn validate(document: &PromptPresetFile) -> Result<(), PromptPresetError>;
pub fn load(runtime_dir: &Path) -> Result<PromptPresetFile, PromptPresetError>;
pub fn save(runtime_dir: &Path, document: &PromptPresetFile) -> Result<(), PromptPresetError>;
```

`PromptPresetError` should implement `Display` and `std::error::Error`, distinguishing I/O, JSON parsing, unsupported document version, and validation errors.

### Built-in preset mapping

Copy the exact prompt strings from `QUICK` in `app/ui/app.js`; keep line content exactly, including placeholder tokens:

| ID | `nameKey` |
|---|---|
| `remove-background` | `qRemoveBg` |
| `snowy-night` | `qSnow` |
| `watercolor` | `qWatercolor` |
| `golden-hour` | `qGolden` |
| `remove-people` | `qNoPeople` |
| `combine-1-2` | `qCombine` |

### Persistence contract

- Path is `runtime_dir.join("presets.json")`; do not use a hard-coded config/runtime location.
- Missing file returns `defaults()` without writing it. First successful Save creates it.
- Existing valid file is authoritative. Do not merge defaults into it, including after the user deletes a built-in or saves an empty list.
- Reject unsupported versions and malformed JSON with an error; leave the source bytes unchanged.
- Validate unique non-empty IDs, non-empty trimmed names/prompts, and `name_key` restricted to the six built-in keys.
- Trim strings only for validation. Preserve prompt contents exactly, including whitespace, newlines, Unicode, and image tokens.
- Serialize and validate before touching the target. Create a temporary file in the same directory, write JSON, call `sync_all()`, then persist it over the destination. Do not remove the previous file first.

Add `tempfile = "3"` to `crates/qil-core/Cargo.toml`. The intended persistence outline is:

```rust
let destination = path(runtime_dir);
let bytes = serde_json::to_vec_pretty(document)?;
std::fs::create_dir_all(runtime_dir)?;
let mut temporary = tempfile::NamedTempFile::new_in(runtime_dir)?;
temporary.write_all(&bytes)?;
temporary.as_file().sync_all()?;
temporary.persist(&destination)?;
```

Map each error into `PromptPresetError`. Export the module via `pub mod prompt_presets;` in `crates/qil-core/src/lib.rs`.

### TDD and test cases

Write tests first and run them to see the missing behavior fail; then implement and rerun:

```powershell
cargo test -p qil-core prompt_presets
```

Use `tempfile::tempdir()` for test storage; never touch real user files. Cover:

1. Missing file returns six defaults and does not create `presets.json`.
2. Round-trip preserves multiline Russian text, quotes, and `<image2>` exactly.
3. Saving over an existing document updates it and re-loads the new document.
4. A removed built-in stays removed on subsequent load.
5. An empty preset vector persists as empty.
6. Malformed JSON errors without changing original bytes.
7. Unknown version errors without changing original bytes.
8. Duplicate IDs and blank names/prompts are rejected.
9. Invalid save does not alter a previously valid file.
10. `path()` derives from the supplied runtime directory.

**Acceptance:** Core persistence is independently tested, preserves user content, rejects bad documents, and cannot overwrite the original with malformed data.

## 5. Task 3 — Expose Load/Save Commands in Tauri

**File:** `app/src-tauri/src/lib.rs`.

Add commands with this contract:

```rust
#[tauri::command]
fn prompt_presets_load(
    settings: State<'_, Settings>,
) -> Result<PromptPresetFile, String>;

#[tauri::command]
fn prompt_presets_save(
    settings: State<'_, Settings>,
    document: PromptPresetFile,
) -> Result<PromptPresetFile, String>;
```

1. Import `PromptPresetFile` and core load/save functions.
2. Use `cfg(&settings).runtime_dir` to derive the file location, honoring `QIL_RUNTIME` and existing configuration.
3. `prompt_presets_load` returns built-ins or the persisted document; `prompt_presets_save` validates and writes through `qil-core`, returning the saved document only on success.
4. Convert core errors to command error strings.
5. Register both functions in `tauri::generate_handler!`.
6. Compile the workspace and later verify calls from the UI.

Frontend invocation signatures:

```js
await invoke("prompt_presets_load");
await invoke("prompt_presets_save", { document: draft });
```

No user-selected file path is accepted from the frontend.

**Acceptance:** Desktop UI can load/save only through the runtime-scoped backend commands.

## 6. Task 4 — Implement the Edit Prompt-Preset UI

**Files:** new `app/ui/prompt-presets.js`, `app/ui/index.html`, `app/ui/app.js`, `app/ui/styles.css`, `app/ui/i18n.js`, new `app/ui/tests/prompt-presets.test.js`.

### UI structure

Replace `<div class="quick" id="quick"></div>` in the Edit section with a compact selector and management control:

```text
[ Choose a prompt preset…            ▾ ] [ Manage button ]
```

Use a native `<select>` for keyboard accessibility. Add a modal editor with:

- `role="dialog"`, `aria-modal="true"`, and heading association;
- preset list;
- name and prompt fields;
- New, Duplicate, Delete, “From current prompt”, Save, and Cancel controls;
- an accessible error region.

Keep it readable at narrow widths by stacking list and form, allowing the dialog body to scroll. Use these element IDs or document any equivalent consistently: `promptPresetSelect`, `promptPresetManage`, `promptPresetsBackdrop`, `promptPresetsDialog`, `promptPresetsList`, `promptPresetName`, `promptPresetPrompt`, `promptPresetNew`, `promptPresetDuplicate`, `promptPresetDelete`, `promptPresetFromCurrent`, `promptPresetsSave`, `promptPresetsCancel`, `promptPresetsError`.

### JavaScript module contract

Follow the IIFE/CommonJS pattern used in `app/ui/generation-batch.js`:

```js
const api = Object.freeze({ create, displayName });
if (typeof module === "object" && module.exports) module.exports = api;
else window.QilPromptPresets = api;
```

Instantiate from `app.js` with dependencies injected:

```js
const promptPresets = window.QilPromptPresets.create({
  invoke,
  getElement: $,
  translate: t,
  getPrompt: () => $("promptPhotos").value,
  applyPrompt: (prompt) => {
    $("promptPhotos").value = prompt;
    updateGenButton();
  },
  notify,
  isBusy: () => state.busy
});
```

Controller methods:

```text
load()                  Promise<void>
render()                void
setDisabled(disabled)   void
isOpen()                boolean
handleKeydown(event)    void
```

The module owns the saved document and edit draft. Do not duplicate the generation-quality `state.preset` or `#presets` behavior.

### User behavior

- The empty select option says “Choose a prompt preset…”.
- Selecting a preset copies only its prompt to `promptPhotos`, focuses the textarea, and resets selection so the same option can be applied again.
- Do not alter quality, aspect ratio, seed, transparency, or reference photos.
- When there are no presets, disable the selector but leave management available.
- Editing begins on a deep copy. Cancel/Escape discards it.
- New creates a preset with a unique `user-` ID and blank fields.
- “From current prompt” creates a preset from `promptPhotos`; disable it when the trimmed current prompt is empty.
- Duplicate copies name/prompt, clears `nameKey`, and assigns a fresh unique ID.
- Delete affects only the draft; deleting the last item and saving an empty array is allowed.
- Switching list entries flushes current form values into the draft before showing the next entry.
- Renaming a built-in clears `nameKey`; changing only its prompt retains `nameKey`.
- Save invokes `prompt_presets_save`. Disable mutation and repeated Save while saving. On success replace the in-memory document and close; on failure keep the draft open and show the error.
- Saving the library must not change the current instruction.
- Apply user-provided text only via `textContent`, `.value`, and safe DOM creation; never interpolate names/prompts into `innerHTML`.
- Use `crypto.randomUUID()` for IDs.

### Error and load behavior

- Load after entering the configured main application, using backend defaults if the file is absent.
- A load error must not block generation.
- Do not silently replace corrupt data with built-ins. Disable Save until an explicit successful retry/load so an error path cannot overwrite the damaged file.
- Show a translated error and provide a way to retry loading from the management control.
- Keep prompt-editor state across `langchange`; rerender labels without changing draft or prompt contents.

### Integration and keyboard handling

- Remove `QUICK`, `renderQuick()`, its boot call, and its language-change call from `app.js`.
- Add `prompt-presets.js` before `app.js` in `index.html`.
- Call controller `render()` from the existing language-change listener.
- Call `setDisabled(state.busy)` in `updateGenButton()` and remove `#quick button` from the busy-lock selector.
- Route key events to the open prompt editor before global generation shortcuts:

  ```js
  if (promptPresets.isOpen()) {
    promptPresets.handleKeydown(e);
    return;
  }
  ```

- Keep Tab/Shift+Tab inside the modal; Escape closes/discards it; Ctrl+Enter in the editor must not start generation. Restore focus to the management button on close.
- When busy, prevent selecting presets or opening/changing the library.

### Localization

Add equivalent EN/RU/ES values to the existing translation tables in `i18n.js` for:

```text
promptPresetChoose
promptPresetsTitle
promptPresetsManage
promptPresetName
promptPresetPrompt
promptPresetNew
promptPresetDuplicate
promptPresetDelete
promptPresetFromCurrent
promptPresetsEmpty
promptPresetsLoadFailed
promptPresetsSaveFailed
promptPresetNameRequired
promptPresetPromptRequired
promptPresetCopyName
```

Reuse existing Save/Cancel keys where appropriate. `nameKey` display should call the current translator, allowing the six built-in names to switch languages.

### UI tests

Write `app/ui/tests/prompt-presets.test.js` with `node:test`, using dependency injection and fake elements/invoke. Run:

```powershell
node --test ui/tests/prompt-presets.test.js
```

Cover at least:

1. Applying a preset preserves exact multiline text.
2. Reapplying the same preset works after selection reset.
3. Cancel leaves the persisted document unchanged.
4. Save failure retains draft and old in-memory document.
5. Successful save replaces the in-memory list.
6. Duplicate gets a new ID and no `nameKey`.
7. Renaming clears `nameKey`, prompt-only edit retains it.
8. Language changes update built-in labels but not user labels or prompt text.
9. Busy blocks apply and editor open/mutations.
10. Load error cannot invoke Save and can be retried.
11. Empty list can be saved.

Use no new UI framework. Add only test helpers needed to verify observable behavior.

**Acceptance:** All six previous quick actions work through the selector; users can manage a durable local library in EN/RU/ES; editor keyboard behavior is correct.

## 7. Task 5 — Generate and Verify App Icons

**Input:** `assets/app-icon.png`.

**Output:** `app/src-tauri/icons/`.

1. Verify the source is suitable for Tauri icon generation (square PNG, RGBA as required by the CLI). Preserve the source file unchanged.
2. If conversion to RGBA is required, use a temporary derived source, not a destructive edit to `assets/app-icon.png`.
3. From `app/`, run the installed Tauri CLI:

   ```powershell
   npx tauri icon "../assets/app-icon.png" --output "src-tauri/icons"
   ```

4. Verify configured outputs exist: `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.ico`.
5. Verify the ICO contains 16, 24, 32, 48, 64, and 256 px layers; 32 px should be first for best Windows development display.
6. Confirm `app/src-tauri/tauri.conf.json` references those generated paths. Change it only if necessary.
7. Build and visually verify window, executable, and installer icons at 16×16 and 32×32 as well as larger sizes. Do not redesign the artwork without user approval.

**Acceptance:** New icon is used by the app window, executable, and installer.

## 8. Task 6 — Documentation and Full Verification

**Files:** `README.md`, `CHANGELOG.md`.

Document that Edit mode supports a user-editable prompt-preset list, that it stores prompt text (with name/ID for organization) and its location is `<runtime_dir>/presets.json`, defaulting to `%LOCALAPPDATA%\\picalum-qwenimage\\presets.json`. Add a changelog entry following the repository's existing format.

Run checks from the repository root:

```powershell
cargo fmt --all -- --check
cargo test --workspace
```

Run frontend tests from `app/`:

```powershell
node --test ui/tests/*.test.js
```

Then build the desktop app and installer from the root using the documented build script:

```powershell
./build.ps1 -Installer
```

Do not run the ignored real-download integration test; it downloads model files and is unrelated.

### Manual desktop QA

- Create: left panel and no visible Description label.
- Edit: selector replaces quick buttons and each built-in inserts its exact old prompt.
- Create/edit/duplicate/delete/cancel/save all behave as specified.
- Changes and deletions survive restart; deleting all presets remains empty after restart.
- Russian multi-line prompts and `<image2>` survive save/reload exactly.
- Changing EN/RU/ES localizes controls and built-in names, not user names/prompts.
- Save failure keeps the dialog/draft; corrupt JSON does not get overwritten by a fallback.
- Busy generation disables preset actions; editor keyboard controls work without triggering generation/cancel.
- Viewer, history, compare, and drag-and-drop continue working with the new layout.
- Verify light and dark themes, default window size, minimum 1060×680, empty list, long names/prompts, and editor scrolling.
- Verify icon in the desktop window, EXE, and installer.

Use an isolated test runtime for corrupt JSON and write-failure QA; do not damage the real user preset file. Capture screenshots of the updated layout and editor as evidence.

### Completion criteria

- All four agreed UI changes are implemented.
- Prompt presets persist safely and survive restart.
- Existing invalid user data cannot be overwritten by a fallback path.
- Workspace and UI tests pass; app/installer build succeeds.
- Manual desktop QA and screenshots are recorded.
- Handoff summarizes changed files, verification outputs, and build locations.
