# Changelog

## 0.1.19 — 2026-10-12

- Publish a Windows x64 installer, standalone desktop application and `qil` CLI for the current feature set, including mask-guided editing from 0.1.18.
- Reorganize the README for everyday users: plain-language feature overview, installer-first getting started, desktop instructions and troubleshooting before developer / CLI details.
- Clearly identify RTX 3060 12 GB as the only tested GPU hardware; real 10 GB cards remain untested and experimental, with no guarantee of successful generation.

## 0.1.18 — 2026-10-10

- Mask-guided editing (beta): **Edit an area** in Edit mode opens a brush editor for the first photo (brush, eraser, size, edge softness, undo/redo, zoom, pan; English, Russian and Spanish). The model sees the area as a red outline and tint and the app composites the result, so pixels outside the painted area are exactly the source's. This is application-level compositing, not native Qwen inpainting.
- Optionally choose another Edit photo as a reference for the painted area (`<image2>`).
- The result keeps the source size and is the only file added to the history; the source file is never changed.
- Core: new `masked_edit` (mask validation and inward-feathered, alpha-aware composite) and `masked_run` (marking, engine run, resampling, publishing) modules; Tauri command `generate_masked_edit` shares the generation lock, cancel and progress with normal runs.
- Prototype scripts for comparing mask guidance strategies: `bench/mask_proto`.
- Fix: the active tool check mark in the mask editor.

## 0.1.15 — 2026-10-10

- Desktop header: larger product name; the Create / Edit / Surprise tabs are larger and moved away from the brand, centered between it and the status controls.

## 0.1.14 — 2026-10-10

- Desktop header: the product name on top, with the version and the GitHub link on one line below it; the model name and profile tag are removed.

## 0.1.13 — 2026-10-10

- Desktop header: show only the product name and the GitHub link; the model name, profile tag and version are no longer displayed.

## 0.1.12 — 2026-10-10

- Desktop header: remove the model name from the brand line; it now shows the profile tag and version.

## 0.1.11 — 2026-10-10

- Desktop: the automatic Draft verification after install or update is removed; the app opens straight away. The test generation is now an on-demand "Check GPU" button in Settings. The architecture check before downloading weights is unchanged.

## 0.1.10 — 2026-10-09

- GPU verification now starts after install or update: the setup status exposes the engine fingerprint (the UI previously never matched a saved result), and a failed or interrupted run is no longer remembered as started, so it is retried on the next launch.

## 0.1.9 — 2026-10-09

- GPU verification card: styled as a centered overlay that no longer collides with the empty-state text, Retry/Cancel/Continue buttons now have labels, and a failure shows the actual error message.

## 0.1.8 — 2026-10-09

- Setup: GPU architecture is now proven by the bundled PyTorch (selected UUID, runtime UUID match, compiled architecture list) before any model weights are planned or downloaded; new GPU capability check step with pending, proven and rejected states in the desktop app and `qil doctor`.

## 0.1.7 — 2026-10-09

- Edit mode: manage reusable prompt presets saved locally in the runtime directory; the tool panel is now on the left.
- Create prompt field: remove the visible Description label while preserving its accessible name.
- Desktop app and NSIS installer: use the supplied app icon.
- CLI: `doctor` enumerates NVIDIA GPUs, stable selection, VRAM recommendations, transient free VRAM, and verification freshness; `setup --gpu-uuid` prevents silent multi-GPU selection.
- Windows NVIDIA policy: 10 GB minimum; Compact at 10–<12 GB and Balanced at 12 GB+, with automatic desktop Draft verification and explicit Skip.

## 0.1.3 — 2026-10-09

- Output settings: choose the generated-image folder and whether to create JSON sidecars; prompt and model metadata are always embedded in PNGs.
- History: clear the in-app history at any time; history is session-only and resets on restart, while saved image and JSON files stay untouched.
- Settings dialog: long output paths truncate so the folder picker remains inside the window.

## 0.1.2 — 2026-10-08

- Generation: split Generate button with a "Generate forever" mode that continues until cancelled.
- Language menu: English, Russian and Spanish are available from a globe button; English remains the default fallback.
- Windows release: desktop executable, NSIS installer and `qil` CLI artifacts.

## 0.1.0 — 2026-09-24

First public release, four days after Qwen-Image-2.1 came out.

- Desktop app (Windows): text-to-image, editing with up to 10 reference photos, "Surprise me", three quality presets, history, English/Spanish UI.
- First-run installer: detects the GPU and driver, checks disk space, installs a pinned ComfyUI, Python 3.12 and PyTorch CUDA 13, downloads the weights with resume and SHA-256 checks.
- `qil` CLI: `doctor`, `setup`, `generate`, `edit`, `surprise`, `bench`, `config`.
- VRAM profiles: balanced (12 GB+, tested) and compact (8–10 GB, experimental).
- Benchmark: 20 tests against gpt-image-2 with every image, prompt and timing published.
