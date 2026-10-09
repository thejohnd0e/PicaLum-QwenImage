# Product Ideas and Roadmap

Ideas discussed for PicaLum QwenImage. “Accepted” means included in the current product direction; “Backlog” means worth considering, but not scheduled.

## Accepted Direction

- **Darkroom workspace:** dark graphite theme by default, with the image as the main focus and editing controls on the right.
- **English first:** English is the default regardless of operating-system language. Russian and Spanish remain available as explicit language choices.
- **Local-first workflow:** prompts and reference photos stay on the PC and are sent only to the local ComfyUI engine.

## Accepted Features

| Feature | Decisions |
|---|---|
| **Before / after inspection** | Available for photo edits with a reference. Side by side is the main view; a wipe divider is an alternative. Fit and 100% zoom are available, with synchronized pan. Preserve each image’s aspect ratio and show native dimensions when they differ. |
| **Finite generation batches** | Add Generate 4 and Generate 8 alongside Generate forever. Generate sequentially, show progress, and allow stopping after the current image or cancelling it immediately. Unlocked seeds vary per image; a locked seed repeats and is disclosed before the run. |
| **Responsive cancellation** | Bound engine and network waits across generation. Recover from dropped WebSocket events by checking prompt history, target cancellation to PicaLum’s prompt ID, and avoid clearing unrelated ComfyUI queue items. Save completed PNGs atomically and use distinct timeout messages. |

## Backlog

### High-value workflow improvements

1. **Restore a complete request:** let users repeat a result with its prompt, mode, reference photos, preset, aspect ratio, transparency setting, and seed.
2. **Persistent image library:** optionally search and filter saved results across app restarts, mark favorites, and distinguish clearing the view from deleting files.
3. **Reference-photo handling:** reorder photos, make the first (canvas) image obvious, insert `<image1>` / `<image2>` references into instructions, and detect missing files before generation.
4. **GPU-specific time estimates:** improve preset estimates from observed runs, separating cold engine startup from warm generation and accounting for image size and reference count.

### Setup and reliability

5. **Repair and verification tools:** offer an explicit integrity check for installed weights and a guided way to repair missing runtime components without starting setup over.
6. **Download controls:** consider pause/cancel during setup while retaining resumable partial files, and report stalled connections clearly.
7. **Broader automated coverage:** add tests for download resume/error cases, desktop command behavior, and user-visible UI flows that can run without a GPU or model download.

### Longer-term exploration

8. **Masks and brush tools** for localized edits.
9. **Live generation preview** while sampling.
10. **Prompt rewriting**, if it can fit the supported VRAM profiles without undermining generation speed.
11. **Additional hardware backends** beyond NVIDIA on Windows, subject to realistic performance and memory testing.

## Product Principles

- Preserve the user's original files; edits create new outputs.
- Keep generated results reproducible through prompt and seed metadata.
- Prefer explicit, recoverable operations over hidden automation, especially around large downloads and saved files.
- Add GPU-dependent claims only when they are backed by measurements on the stated hardware.
