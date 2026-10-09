# Changelog

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
