# Output Settings and PNG Metadata Design

## Goal

Give users a small settings dialog for choosing where generated images are saved and whether adjacent JSON sidecars are created, while always embedding interoperable prompt and model metadata inside each PNG.

## Decisions

- New installations default to `Runtime\ComfyUI\output`.
- Existing installations using the legacy default `Pictures\PicaLum QwenImage` migrate once to `Runtime\ComfyUI\output` without moving or deleting existing files.
- A user-selected custom output directory remains selected across restarts.
- JSON sidecars default to enabled and can be disabled in settings.
- Embedded PNG metadata is always enabled and is not controlled by the JSON toggle.
- Explicit CLI `--out` continues to override the configured output directory for that command only.
- The existing **Open Folder** button always opens the currently configured output directory.

## Settings UI

Add a 28 x 26 px gear button immediately beside the language button in the top bar. It uses the existing neutral icon-button style and is shown only in the main application, not during first-run installation.

The gear opens a centered modal dialog rather than a small popover. A modal gives the output path enough width and provides predictable keyboard and focus behavior.

The dialog contains:

- **Output folder**: the current path in a truncated monospace field and a **Change...** button that opens the native folder picker.
- **Create JSON metadata files**: a checkbox controlling adjacent `.json` sidecars.
- A short fixed note: the prompt and model are always embedded in each PNG for compatibility with metadata readers.
- **Cancel** and **Save** buttons.

Opening the dialog copies current settings into a draft. Choosing a folder updates only the draft. **Save** persists both values atomically. **Cancel**, clicking the backdrop, or pressing Escape discards the draft. The dialog traps focus while open and returns focus to the gear button when closed.

Settings cannot be saved during generation. The gear button is disabled while generation is active, and the Rust command independently rejects a settings update if generation is active so UI timing cannot create a race.

All labels and messages are added in English, Russian, and Spanish through the existing `i18n.js` dictionary.

## Configuration

Keep `Config.output_dir: PathBuf` because output is now intentionally configurable. Add:

```rust
#[serde(default = "default_save_json")]
pub save_json: bool,

#[serde(default)]
pub config_version: u32,
```

The current schema version is `1`. For a legacy configuration with version `0`:

- If `output_dir` equals the legacy platform default `Pictures\PicaLum QwenImage`, replace it with `runtime_dir/ComfyUI/output`.
- Preserve any other output path as an intentional custom value.
- Set `save_json` to `true`.
- Set `config_version` to `1` and persist the migrated config.
- Never inspect, move, copy, or delete files in the old output directory.

Fresh defaults use `runtime_dir/ComfyUI/output`, `save_json = true`, and `config_version = 1`.

The CLI keeps `qil config --output-dir` and gains matching visibility for `save_json` only if a CLI setter is needed during implementation; the desktop feature does not require a new CLI flag. `qil generate --out` remains the per-run override.

## Tauri Commands

Add a serializable settings response containing `outputDir` and `saveJson`.

- `app_settings() -> AppSettings` returns the current values.
- `pick_output_folder()` opens a native directory picker titled for generated images.
- `save_app_settings(output_dir: String, save_json: bool) -> Result<AppSettings, String>` validates, creates, and persists the directory before updating shared state.

`save_app_settings` performs operations in this order:

1. Reject an empty or non-absolute path.
2. Reject the update while generation is active.
3. Create the selected directory if needed.
4. Save a cloned `Config` to disk.
5. Add the directory to the runtime asset-protocol scope.
6. Replace the in-memory config only after all required operations succeed.

The existing `generate`, `list_outputs`, `output_dir`, startup asset scope, and **Open Folder** flow continue to read `Config.output_dir`.

After a successful save, the frontend updates the displayed path and reloads history from the new directory. Existing files in the previous directory are left untouched.

## Generation Storage

Use ComfyUI's `PreviewImage` output node so its engine-side artifact is temporary rather than a permanent duplicate in `ComfyUI/output`. Continue retrieving the result through `/history` and `/view`.

Before writing the final PNG, inject metadata chunks without decoding or re-encoding pixels. This preserves RGBA data, compression, and existing ComfyUI chunks.

The final save sequence is:

1. Download and validate the PNG returned by ComfyUI.
2. Insert the approved textual metadata chunks before `IEND`.
3. Write the PNG to the selected output directory or explicit CLI destination.
4. If `save_json` is enabled, write the existing structured JSON beside it.
5. Best-effort remove the local ComfyUI preview after validating that its reported path is inside `Runtime\ComfyUI\temp`.
6. Return `GenResult` only after required PNG writing succeeds.

## PNG Metadata

Use uncompressed UTF-8 `iTXt` chunks so prompts in Cyrillic and other scripts survive unchanged.

Write `parameters` for `eagle-png-metadata` as a human-readable payload:

```text
<prompt>

Model: Qwen-Image-2.1
Seed: <seed>
Steps: <steps>
Size: <width>x<height>
Preset: <preset>
```

Write `XML:com.adobe.xmp` for Prompt Lens with:

- `Iptc4xmpExt:AIPromptInformation`: the original user prompt.
- `Iptc4xmpExt:AISystemUsed`: `PicaLum QwenImage`.
- `Iptc4xmpExt:AISystemVersionUsed`: `Qwen-Image-2.1`.
- `Iptc4xmpExt:DigitalSourceType`: the IPTC trained-algorithmic-media value.

Escape XML attribute values correctly. Do not put reference image paths, runtime paths, or other local filesystem details into embedded PNG metadata.

Keep the JSON sidecar schema unchanged when enabled because it contains richer application data and supports the current history view.

## History Without JSON

When JSON sidecars are disabled, newly generated images still appear immediately because the frontend receives `GenResult` and request data. After restart, `list_outputs` returns the PNG with `meta: null`; the history thumbnail remains usable, while prompt/seed actions are unavailable unless PNG metadata reading is added in a separate feature.

This feature writes interoperable PNG metadata but does not add a PNG metadata reader to PicaLum QwenImage. Reading metadata back into history is explicitly outside this scope.

## Error Handling

- Folder validation or persistence errors leave both in-memory and on-disk settings unchanged and keep the modal open with an error message.
- A malformed engine response or PNG metadata injection failure returns a generation error and does not write a partial final PNG.
- JSON writing remains best-effort, matching current behavior. Disabling JSON skips the write entirely.
- Preview cleanup accepts only `type = "temp"`, normal relative subfolder components, and a plain filename. It never deletes outside `Runtime\ComfyUI\temp`.
- Failure to remove a validated temporary ComfyUI preview does not fail a completed generation; ComfyUI cleans its temp directory on startup and shutdown.

## Testing

Rust unit tests cover:

- Fresh configuration defaults.
- One-time migration of the legacy Pictures default without touching files.
- Preservation of a custom output directory.
- `save_json` default and serialization.
- `PreviewImage` graph output.
- Safe local preview path resolution and rejection of traversal paths.
- UTF-8 `iTXt` injection, CRC validity, XMP escaping, and preservation of existing PNG chunks and image pixels.
- Conditional JSON writing.
- Settings validation and atomic state replacement through extracted pure helpers where direct Tauri command tests are impractical.

Manual desktop checks cover:

- Gear placement beside language selection in light and dark themes.
- Keyboard focus, Escape, backdrop cancel, and Save behavior.
- Folder picker, history reload, displayed output path, and **Open Folder** consistency.
- JSON enabled and disabled generations.
- Reading the embedded prompt/model with both `eagle-png-metadata` and Prompt Lens.
- No new files written to the legacy Pictures directory after migration.
