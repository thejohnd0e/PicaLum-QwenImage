# Output Settings and PNG Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add persistent output settings, optional JSON sidecars, and interoperable prompt/model metadata inside every generated PNG while eliminating permanent ComfyUI duplicates.

**Architecture:** `Config` remains the source of truth for output folder and JSON policy, with a one-time migration from the old Pictures default to `Runtime\ComfyUI\output`. ComfyUI produces a temporary preview; `qil-core` injects UTF-8 PNG text chunks and writes one final PNG to the configured or explicit CLI destination. A small Tauri-backed settings modal updates configuration, history, asset scope, and the existing **Open Folder** behavior together.

**Tech Stack:** Rust 2021, Tokio, Reqwest, Serde JSON, `crc32fast`, Tauri 2, plain HTML/CSS/JavaScript, ComfyUI HTTP API.

**Spec:** `docs/superpowers/specs/2026-10-08-output-settings-and-png-metadata-design.md`

## Global Constraints

- Fresh installations default to `Runtime\ComfyUI\output`.
- Migrate the exact legacy Pictures default once without inspecting, moving, copying, or deleting existing images.
- Preserve any previously configured custom output directory.
- JSON sidecars default to enabled and are optional; embedded PNG metadata is always written.
- Keep explicit CLI `--out` behavior.
- Keep one-stage-at-a-time model loading and engine child-process lifecycle binding.
- Keep reference upload filenames unique and rotate source images only through copies.
- Do not put local file paths into embedded PNG metadata.
- Do not manually edit `Cargo.lock`; let Cargo update it when adding `crc32fast`.
- Do not add a frontend framework or JavaScript test dependency.
- Use `npm ci` if frontend dependencies are installed.
- Treat Qwen model weights as non-commercial under the Qwen Research License.

---

### Task 1: Version and migrate output configuration

**Files:**
- Modify: `crates/qil-core/src/config.rs`
- Test: `crates/qil-core/src/config.rs` test module

**Interfaces:**
- Preserve `Config.output_dir: PathBuf`.
- Add `Config.save_json: bool` and `Config.config_version: u32`.
- Add `CURRENT_CONFIG_VERSION: u32 = 1` and a pure migration helper returning whether persistence is needed.

- [ ] **Step 1: Write failing tests for defaults and migration**

  Add tests proving:

  ```rust
  let cfg = Config::default();
  assert_eq!(cfg.output_dir, cfg.comfy_dir().join("output"));
  assert!(cfg.save_json);
  assert_eq!(cfg.config_version, CURRENT_CONFIG_VERSION);
  ```

  Build version-0 fixtures and assert that the exact legacy Pictures path migrates to `comfy_dir()/output`, while `D:\Art\Qwen` remains unchanged. The migration test must compare paths only and must not create or inspect either directory.

- [ ] **Step 2: Run focused tests and verify they fail**

  Run: `cargo test -p qil-core config::tests -- --nocapture`

  Expected: compilation or assertion failure because the new fields and migration do not exist.

- [ ] **Step 3: Implement config schema and migration**

  Add defaults equivalent to:

  ```rust
  const CURRENT_CONFIG_VERSION: u32 = 1;

  fn default_save_json() -> bool { true }

  #[serde(default = "default_save_json")]
  pub save_json: bool,
  #[serde(default)]
  pub config_version: u32,
  ```

  Initialize fresh `output_dir` from the already-created `runtime_dir`:

  ```rust
  output_dir: runtime_dir.join("ComfyUI").join("output")
  ```

For version `0`, replace only the exact legacy platform default `Pictures\PicaLum QwenImage`, set `save_json = true`, set version `1`, and persist the migrated config best-effort. Apply environment overrides after deserialization without interpreting arbitrary custom paths as defaults.

- [ ] **Step 4: Run focused tests**

  Run: `cargo test -p qil-core config::tests -- --nocapture`

  Expected: PASS.

- [ ] **Step 5: Commit the config migration**

  ```powershell
  git add crates/qil-core/src/config.rs
  git commit -m "feat: add persistent output settings"
  ```

### Task 2: Add lossless PNG textual metadata injection

**Files:**
- Create: `crates/qil-core/src/png_metadata.rs`
- Modify: `crates/qil-core/src/lib.rs`
- Modify: `crates/qil-core/Cargo.toml`
- Test: `crates/qil-core/src/png_metadata.rs` test module

**Interfaces:**
- Produce `GenerationMetadata<'a>` with prompt, seed, steps, dimensions, and preset.
- Produce `embed_generation_metadata(png: &[u8], meta: &GenerationMetadata<'_>) -> Result<Vec<u8>, String>`.
- Preserve every existing PNG byte except insertion of two valid `iTXt` chunks immediately before `IEND`.

- [ ] **Step 1: Write failing tests for UTF-8 metadata and chunk preservation**

  Use an in-memory minimal PNG fixture. Assert that the returned chunk stream contains:

  ```text
  iTXt keyword: parameters
  iTXt keyword: XML:com.adobe.xmp
  ```

  Assert the `parameters` value contains a Cyrillic prompt, `Model: Qwen-Image-2.1`, seed, steps, size, and preset. Assert XMP contains escaped values for a prompt such as `кот & <луна>`. Verify all original chunks remain byte-identical and in their original order, and verify CRCs for both inserted chunks.

- [ ] **Step 2: Run the focused test and verify it fails**

  Run: `cargo test -p qil-core png_metadata -- --nocapture`

  Expected: compilation failure because the module does not exist.

- [ ] **Step 3: Add the CRC dependency and module boundary**

  Add `crc32fast = "1"` to `crates/qil-core/Cargo.toml` and export the module internally from `lib.rs`. Do not manually edit `Cargo.lock`.

- [ ] **Step 4: Implement strict PNG traversal and iTXt construction**

  Validate the eight-byte PNG signature. Traverse each chunk as `length + type + data + crc`, rejecting truncated lengths and missing `IEND`. Construct uncompressed UTF-8 `iTXt` data as:

  ```text
  keyword\0 compression_flag=0 compression_method=0 language_tag\0 translated_keyword\0 UTF-8 text
  ```

  Calculate CRC over the four-byte `iTXt` type plus chunk data. Insert both complete chunks directly before the original `IEND` chunk.

- [ ] **Step 5: Implement payload builders**

  Build `parameters` as:

  ```text
  <prompt>

  Model: Qwen-Image-2.1
  Seed: <seed>
  Steps: <steps>
  Size: <width>x<height>
  Preset: <preset>
  ```

Build XMP with `Iptc4xmpExt:AIPromptInformation`, `AISystemUsed="PicaLum QwenImage"`, `AISystemVersionUsed="Qwen-Image-2.1"`, and the IPTC trained-algorithmic-media `DigitalSourceType`. Escape `&`, `<`, `>`, `"`, and `'`; never include reference paths or runtime paths.

- [ ] **Step 6: Run focused and core tests**

  Run: `cargo test -p qil-core png_metadata -- --nocapture`

  Expected: PASS.

  Run: `cargo test -p qil-core`

  Expected: PASS.

- [ ] **Step 7: Commit PNG metadata support**

  ```powershell
  git add crates/qil-core/Cargo.toml Cargo.lock crates/qil-core/src/lib.rs crates/qil-core/src/png_metadata.rs
  git commit -m "feat: embed generation metadata in png files"
  ```

### Task 3: Eliminate persistent engine duplicates and honor JSON policy

**Files:**
- Modify: `crates/qil-core/src/generate.rs`
- Test: `crates/qil-core/src/generate.rs` test module

**Interfaces:**
- Preserve `generate::run`, `GenRequest`, and `GenResult` public signatures.
- Consume `Config.output_dir`, `Config.save_json`, and `png_metadata::embed_generation_metadata`.
- Preserve explicit folder and exact `.png` output behavior.
- Produce a private `local_preview_path(cfg: &Config, image: &Value) -> Option<PathBuf>` that resolves only safe `type = "temp"` files beneath `cfg.comfy_dir().join("temp")`.

- [ ] **Step 1: Write failing graph and save-policy tests**

  Extend `build_graph` tests to assert:

  ```rust
  assert_eq!(graph["save"]["class_type"], "PreviewImage");
  assert!(graph["save"]["inputs"].get("filename_prefix").is_none());
  ```

  Extract a small pure helper for deciding whether to produce sidecar bytes and test both `save_json = true` and `false`. Test `local_preview_path` with a valid temp filename and reject `type = "output"`, absolute paths, `..`, and separators inside `filename`.

- [ ] **Step 2: Run focused tests and verify they fail**

  Run: `cargo test -p qil-core generate::tests -- --nocapture`

  Expected: failure because the graph still uses `SaveImage` and JSON is unconditional.

- [ ] **Step 3: Switch ComfyUI output to `PreviewImage`**

  Keep the node id `save` so progress-stage mapping remains unchanged, but change its class and inputs to:

  ```json
  {"class_type":"PreviewImage","inputs":{"images":["decode",0]}}
  ```

  Keep history parsing generic and continue using returned `filename`, `subfolder`, and `type` for `/view`.

- [ ] **Step 4: Embed metadata before writing the final PNG**

  Decode dimensions from downloaded bytes as today, construct `GenerationMetadata` from the original user prompt and resolved generation values, and replace the final `tokio::fs::write(&file, &bytes)` input with the metadata-enriched byte vector. A malformed PNG or injection failure returns an error before a partial final file is written.

- [ ] **Step 5: Gate JSON sidecar writing**

  Preserve the existing JSON schema and best-effort write, but execute it only when `cfg.save_json` is true:

  ```rust
  if cfg.save_json {
      let _ = tokio::fs::write(file.with_extension("json"), serde_json::to_vec_pretty(&meta).unwrap()).await;
  }
  ```

- [ ] **Step 6: Remove only the validated local preview**

  After the final PNG write and optional JSON write, call `local_preview_path`. If it returns a path, remove that file best-effort with `tokio::fs::remove_file`. A missing file is expected when attached to an external engine and must not fail generation. Never derive a deletion target from unvalidated absolute or parent-directory components.

- [ ] **Step 7: Run focused and workspace Rust tests**

  Run: `cargo test -p qil-core generate::tests -- --nocapture`

  Expected: PASS.

  Run: `cargo test -p qil-core -p qil-cli`

  Expected: PASS.

- [ ] **Step 8: Commit generation storage changes**

  ```powershell
  git add crates/qil-core/src/generate.rs
  git commit -m "fix: save one metadata-rich generation result"
  ```

### Task 4: Add atomic Tauri settings commands

**Files:**
- Modify: `app/src-tauri/src/lib.rs`
- Test: `app/src-tauri/src/lib.rs` test module for extracted helpers

**Interfaces:**
- Produce `AppSettings { output_dir: String, save_json: bool }` serialized as camelCase.
- Produce commands `app_settings`, `pick_output_folder`, and `save_app_settings`.
- Keep `output_dir`, `list_outputs`, generation, and asset protocol scope driven by the same `Config.output_dir`.

- [ ] **Step 1: Write failing path validation tests**

  Extract `fn validated_output_dir(path: &str) -> Result<PathBuf, String>` and test that an empty or relative path is rejected while an absolute Windows path is accepted under `#[cfg(windows)]`.

- [ ] **Step 2: Run the app library tests and verify failure**

Run: `cargo test -p qil-core --lib`

  Expected: compilation failure because the helper and settings commands do not exist.

- [ ] **Step 3: Implement settings DTO and read command**

  Add:

  ```rust
  #[derive(Serialize)]
  #[serde(rename_all = "camelCase")]
  struct AppSettings {
      output_dir: String,
      save_json: bool,
  }
  ```

  `app_settings` returns a snapshot from the `RwLock<Config>`.

- [ ] **Step 4: Implement output folder picker**

  Add a command using the existing dialog plugin with title `Choose where generated images are saved`. Return `Option<String>` exactly as the model-folder picker does.

- [ ] **Step 5: Implement atomic settings persistence**

  `save_app_settings` rejects updates while `BUSY` is true, validates and creates the directory, clones the current config, updates `output_dir` and `save_json`, saves the clone, adds the selected directory to `app.asset_protocol_scope()`, then replaces the in-memory config. A failure before replacement leaves current settings unchanged.

- [ ] **Step 6: Register commands and unify path consumers**

  Add all three commands to `generate_handler!`. Confirm `generate`, `list_outputs`, `output_dir`, and startup scope all use `Config.output_dir` and create the configured directory where appropriate.

- [ ] **Step 7: Run app and core tests**

Run: `cargo test -p qil-core --lib`

  Expected: PASS.

  Run: `cargo test -p qil-core -p qil-cli`

  Expected: PASS.

- [ ] **Step 8: Commit backend settings support**

  ```powershell
  git add app/src-tauri/src/lib.rs
  git commit -m "feat: expose desktop output settings"
  ```

### Task 5: Build the accessible settings modal

**Files:**
- Modify: `app/ui/index.html`
- Modify: `app/ui/styles.css`
- Modify: `app/ui/app.js`
- Modify: `app/ui/i18n.js`

**Interfaces:**
- Consume Tauri commands `app_settings`, `pick_output_folder`, and `save_app_settings`.
- Keep the existing `output_dir` command as the source for **Open Folder**.
- Do not add dependencies or a frontend build step.

- [ ] **Step 1: Add semantic modal markup and gear button**

  Put a small icon button immediately before the existing `.lang` element. Add a hidden backdrop containing a `role="dialog"`, `aria-modal="true"`, labelled heading, output path row, JSON checkbox, compatibility note, inline error region, and Cancel/Save buttons.

- [ ] **Step 2: Add localized strings**

  Add English, Russian, and Spanish strings for settings title, output folder, JSON sidecars, embedded PNG note, Change, Cancel, Save, invalid folder, busy-generation rejection, and save failure. Keep all three dictionaries structurally aligned.

- [ ] **Step 3: Style the modal in the existing visual language**

  Reuse `--panel`, `--sunken`, borders, 2-3 px radii, Segoe UI, and monospace path styling. Keep the dialog around 500 px wide with a mobile-safe `max-width: calc(100vw - 32px)`. Do not introduce cards, gradients, oversized headings, or pill controls.

- [ ] **Step 4: Implement draft, save, and cancel behavior**

  On open, fetch `app_settings`, populate a draft, and focus the first control. The native picker changes only the draft. Save invokes `save_app_settings`; on success close the modal, refresh `outDirTxt`, and call `loadHistory()`. Cancel, Escape, and backdrop click discard the draft. Preserve focus inside the modal on Tab/Shift+Tab and return focus to the gear button on close.

- [ ] **Step 5: Prevent settings races**

  Disable the gear while `state.busy` is true and update it anywhere generation busy state changes. If Rust still returns the busy error, show it in the modal without closing or losing the draft.

- [ ] **Step 6: Verify the Open Folder connection**

  Keep the existing handler:

  ```javascript
  invoke("open_path", { path: await invoke("output_dir") })
  ```

  After settings save, `output_dir` must resolve to the selected path without frontend path duplication.

- [ ] **Step 7: Run JavaScript syntax checks**

  Run: `node --check app/ui/app.js`

  Expected: PASS.

  Run: `node --check app/ui/i18n.js`

  Expected: PASS.

- [ ] **Step 8: Commit the settings UI**

  ```powershell
  git add app/ui/index.html app/ui/styles.css app/ui/app.js app/ui/i18n.js
  git commit -m "feat: add output settings dialog"
  ```

### Task 6: Align CLI and documentation

**Files:**
- Modify: `crates/qil-cli/src/main.rs`
- Modify: `README.md`
- Test: existing CLI tests or new `#[cfg(test)]` module in `crates/qil-cli/src/main.rs`

**Interfaces:**
- Keep `qil config --output-dir` because output is intentionally configurable.
- Keep `qil generate --out` as a per-command override.
- Report `saveJson` in serialized config output; no new setter is required for this desktop-focused feature.

- [ ] **Step 1: Write or update CLI default-output assertions**

  Test the pure output selection expression by extracting a helper if needed: no `--out` uses `cfg.output_dir`, while an explicit PNG or directory path wins.

- [ ] **Step 2: Update CLI help and doctor output**

Replace `default: your Pictures\PicaLum QwenImage` with wording that references the configured output folder. Keep the `doctor` output line showing the active directory.

- [ ] **Step 3: Update README privacy and storage wording**

  State that prompt/model metadata is embedded in every PNG, JSON sidecars are optional in desktop settings, and the default folder is the bundled runtime output but can be changed.

- [ ] **Step 4: Run CLI and core tests**

  Run: `cargo test -p qil-core -p qil-cli`

  Expected: PASS.

- [ ] **Step 5: Commit CLI and documentation changes**

  ```powershell
  git add crates/qil-cli/src/main.rs README.md
  git commit -m "docs: describe output and metadata settings"
  ```

### Task 7: End-to-end verification

**Files:**
- Inspect: `crates/qil-core/src/config.rs`
- Inspect: `crates/qil-core/src/png_metadata.rs`
- Inspect: `crates/qil-core/src/generate.rs`
- Inspect: `app/src-tauri/src/lib.rs`
- Inspect: `app/ui/index.html`
- Inspect: `app/ui/styles.css`
- Inspect: `app/ui/app.js`
- Inspect: `app/ui/i18n.js`

- [ ] **Step 1: Run CI-equivalent checks**

  ```powershell
  cargo test -p qil-core -p qil-cli
  cargo clippy -p qil-core -p qil-cli -- -D warnings
  node --check app/ui/app.js
  node --check app/ui/i18n.js
  ```

  Expected: all commands pass.

- [ ] **Step 2: Run desktop host checks**

Run: `cargo test -p qil-core --lib`

  Expected: PASS.

Run: `cargo clippy -p qil-core --lib -- -D warnings`

  Expected: PASS.

- [ ] **Step 3: Verify the settings modal manually**

  Run the development app. Check gear placement beside language selection at minimum window size, both color schemes, keyboard focus loop, Escape/backdrop cancel, folder picker, validation errors, and persistence after restart.

- [ ] **Step 4: Verify storage behavior manually**

  Generate with JSON enabled and confirm one PNG plus one JSON in the selected directory. Generate with JSON disabled and confirm only one PNG. Confirm no new persistent PNG appears in `ComfyUI/output` unless that directory itself is selected, and no new file appears in the legacy Pictures directory after migration.

- [ ] **Step 5: Verify history and folder actions**

  Change output directory, confirm history reloads from it, generate an image, restart the app, and confirm the image remains visible. Confirm **Open Folder** and **Show in Folder** resolve to the current configured directory and file.

- [ ] **Step 6: Verify metadata interoperability**

Open a generated PNG in `eagle-png-metadata` and confirm the full `parameters` block. Open the same PNG in Prompt Lens and confirm clean prompt, `PicaLum QwenImage`, and `Qwen-Image-2.1` fields, including a Cyrillic prompt.

- [ ] **Step 7: Review the final diff**

  Confirm changes are limited to the approved settings, storage, metadata, tests, and documentation. Do not modify existing images or generated output during automated tests.
