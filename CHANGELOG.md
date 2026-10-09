# Changelog

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
