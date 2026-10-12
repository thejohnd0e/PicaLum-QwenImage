<div align="center">

**Type a prompt or drop up to 10 photos: Qwen-Image-2.1 renders it on your 12 GB GPU in about 35 s, and nothing leaves your PC.**

[![Status](https://img.shields.io/badge/status-beta-d29922?style=flat-square)](CHANGELOG.md)
[![Windows](https://img.shields.io/badge/windows-10%20%7C%2011-0078D4?style=flat-square&logo=windows&logoColor=white)](#install)
[![CI](https://img.shields.io/github/actions/workflow/status/thejohnd0e/PicaLum-QwenImage/ci.yml?branch=master&style=flat-square&label=CI)](https://github.com/thejohnd0e/PicaLum-QwenImage/actions/workflows/ci.yml)
[![Tested GPU](https://img.shields.io/badge/tested%20GPU-RTX%203060%2012%20GB-76B900?style=flat-square&logo=nvidia&logoColor=white)](#gpu-support-and-verification)
[![Model](https://img.shields.io/badge/model-Qwen--Image--2.1-615CED?style=flat-square)](https://huggingface.co/Qwen/Qwen-Image-2.1)
[![Code license](https://img.shields.io/badge/code%20license-MIT-green?style=flat-square)](LICENSE)

<p><img src="assets/Screenshot%20-%202026-10-12%2017.44.28.png" alt="PicaLum QwenImage — Create mode with prompt, quality, aspect ratio and seed controls" width="100%"/></p>
<p><img src="assets/Screenshot%20-%202026-10-12%2017.44.39.png" alt="PicaLum QwenImage — Edit mode with reference photos, editing instruction and prompt presets" width="100%"/></p>

<br/>

<table>
<tr>
<td align="center"><strong>12 GB</strong><br/><sub>VRAM, every stage fully on GPU</sub></td>
<td align="center"><strong>35 s</strong><br/><sub>per 1024² image, RTX 3060</sub></td>
<td align="center"><strong>10</strong><br/><sub>reference photos per edit</sub></td>
<td align="center"><strong>20</strong><br/><sub>tests vs gpt-image-2, all published</sub></td>
<td align="center"><strong>0</strong><br/><sub>uploads: prompts and photos stay on your PC</sub></td>
</tr>
</table>

**🛠️ Build it:** [one command](#install) &nbsp;•&nbsp; **⭐ [Star it](https://github.com/thejohnd0e/PicaLum-QwenImage/stargazers)** if it runs on your card: that is how other people with small GPUs find it.

</div>

---

Qwen-Image-2.1 came out on September 20, 2026. It is the first open-weight image model that is genuinely good **and** small: a 7B generator, where most good open models are 10–20B. This repo is the missing piece for people who don't own a 24 GB card: a one-click desktop app and a CLI that install it, fit it into 12 GB of VRAM, and get out of your way.

<sub>Unofficial. Not affiliated with Alibaba or the Qwen team. The model weights are non-commercial; see [License](#license).</sub>

**User guide:** [Install](#install) · [Desktop controls](#desktop-user-guide) · [Edit an area](#mask-guided-editing) · [GPU requirements](#gpu-support-and-verification) · [CLI reference](#cli-command-and-option-reference) · [Troubleshooting](#troubleshooting)

## What it does

| | Feature | Details |
|---|---|---|
| **Create** | Text → image | Draft 1024² · Standard 1.6 MP · Max native 2K, seven aspect ratios, lockable seed |
| **Transparency** | Native RGBA | Real alpha channel from the model, no background-removal tricks |
| **Edit** | 1–10 reference photos | Relight, restyle, remove objects or people, rewrite signs, combine subjects; the first photo is the canvas. Save and manage reusable prompt presets in the app. |
| **Edit an area** | Brush mask (beta) | Paint the area to change on the first photo and describe the change; optionally pick another photo as the content for that area. Everything outside the painted area stays pixel-identical to the source. The result keeps the source size. See [how it works](#mask-guided-editing) |
| **Surprise me** | Idea generator | Style × subject × place × light, or a curated idea; optional theme |
| **Series** | 1, 4, 8 or continuous generation | Sequential images, random or locked seeds, immediate cancel; finite series can stop after the current image |
| **Compare** | Before / after viewer | Regular photo edits: result-only, side by side or a wipe slider, fit / 100% zoom and synchronized panning |
| **Prompt presets** | Reusable editing instructions | Six built-in prompts; create, duplicate, rename, edit and delete your own |
| **Setup** | One-command build + one-time install | Pinned ComfyUI, Python 3.12, PyTorch CUDA 13, weights with resume and SHA-256 checks |
| **CLI** | `qil` | `doctor` · `setup` · `generate` · `edit` · `surprise` · `bench` · `config` |
| **Privacy** | Local | Prompts and photos only ever go to the engine on `127.0.0.1`. Prompt and model metadata are embedded in every PNG; adjacent JSON sidecars are optional in desktop settings |
| **History** | Session-only | Clear history in the app or restart to reset the history view; saved PNG and JSON files stay in the output folder |
| **Languages** | English · Русский · Español | Switch from the language button in the header; the choice is remembered |

## Desktop user guide

The instructions below use the English button names. The same controls are available in Russian and Spanish. Jump to [Install](#install) if this is your first launch.

### First launch and engine controls

1. On the setup screen, select your NVIDIA GPU if more than one is listed. **Auto** recommends a VRAM profile; **Manual** lets you select Balanced or Compact. **The tested hardware is an RTX 3060 12 GB. Real 10–<12 GB cards have not been tested; Compact on those cards is experimental and generation is not guaranteed.** Manual selection does not bypass the minimum VRAM, driver or architecture checks.
2. Use **Models folder → Change…** to choose another disk or reuse existing weights. The expected layout is `diffusion_models/`, `text_encoders/` and `vae/` inside that folder. Setup shows the required files, download size and free disk space.
3. Press **Install**. Watch per-file progress, download speed and checksum verification; expand **Details** for installation logs. If installation fails, correct the reported problem and use **Retry install**; interrupted downloads can resume.
4. When installation finishes, the app opens the workspace and starts the engine. Subsequent launches reuse the installed runtime. Wait for **Engine ready** before generating; engine startup and the first model load can take longer than later runs.

The header shows engine status, the selected GPU and its current used / total VRAM. **Stop engine** releases the app-owned engine's GPU memory; **Start engine** starts it again. These controls are unavailable during generation. If the app attaches to an already running engine, it shows **Engine ready (external)** and does not offer to stop that process: stop it from the program that started it.

The gear button opens **Output settings**, including the optional **Check GPU** test. The language button selects English, Russian or Spanish; English is the initial default and your selection is saved. The header also displays the app version and a link to this repository.

### Create an image

1. Choose **Create** and describe the subject, setting, style and lighting in the prompt field.
2. Select **Draft**, **Standard** or **Max**. Standard is the initial default. The time estimates shown in the app are measurements for an RTX 3060, not a promise for every GPU.
3. Select **Format**: `1:1`, `4:3`, `3:4`, `3:2`, `2:3`, `16:9` or `9:16`.
4. Optionally enable **Transparent background** for native RGBA PNG output. This checkbox applies to Create; it is not a background-removal filter for existing photos.
5. Leave **Seed → Lock** off for a random seed, or enable it and enter a number to reuse a seed. Keep the prompt, quality and format the same when comparing runs with a fixed seed; a seed alone does not preserve an image when other inputs change.
6. Press **Generate** or **Ctrl+Enter**. The result is saved automatically; no separate Save image action is needed.

Quality has different size settings for creation and editing:

| Quality | Steps | Create size | Regular Edit resolution setting | Approx. Create / Edit time on RTX 3060 |
|---|---:|---|---:|---|
| Draft | 20 | About 1 MP; 1024 × 1024 at `1:1` | 1024 px | 35 s / 50 s |
| Standard | 30 | About 1.6 MP | 1280 px | 1.5 min / 2.5 min |
| Max | 40 | Native 2K presets; 2048 × 2048 at `1:1` | 1536 px | 7 min / 5 min |

Create dimensions depend on the chosen aspect ratio. Regular Edit uses the first photo as its canvas, so the Create format buttons do not set its size; editing resolution is not a guarantee of preserving the source's pixel dimensions. [Mask-guided editing](#mask-guided-editing) additionally resamples and composites to keep the source size.

### Edit and combine photos

1. Choose **Edit**. Click the reference area / **+** to select photos, or drag files onto the app. Dropping files also switches to Edit. Supported reference formats are PNG, JPG/JPEG, WebP and BMP; the limit is **10 photos**.
2. Add the canvas photo first. Thumbnail numbers match the tokens `<image1>`, `<image2>`, … in your instruction. Additional photos provide subjects, style or other reference material. For example: `Place the dog from <image2> next to the bicycle in <image1>, matching lighting and perspective.`
3. Use **↻** on a thumbnail to rotate it 90° clockwise, or **×** to remove it from the reference list. Rotation creates a separate copy; the original file is unchanged. To change the canvas, remove / re-add references in the desired order.
4. Write the change you want and what should stay the same, or choose a saved prompt. Select quality and optionally lock a seed, then press **Edit photo** / **Generate with … photos**.

Ordinary photo editing asks the model to preserve the first photo's framing, but it can still change details outside your intended edit. Use **Edit an area** when pixels outside a selected area must remain unchanged.

### Save and manage editing prompts

**Choose a prompt preset…** replaces the current Edit instruction with the selected prompt; you can then customize the text before generating. It does not start generation or change references, quality or seed.

The six initial presets are **Remove background**, **Snowy night**, **Watercolor**, **Golden hour**, **Remove people** and **Combine 1 + 2**. They are ordinary editable model instructions, not separate processing tools or guaranteed effects.

Open **Manage presets** to:

- **New**: create a named prompt.
- **From current prompt**: add the current Edit instruction as a new preset.
- **Duplicate**: make a copy of the selected preset.
- Edit its **Name** and **Prompt**, or **Delete** it.
- Press **Save** to persist all changes. Every remaining preset needs a non-empty name and prompt. **Cancel**, Escape or closing the dialog without saving discards the changes.

Custom presets persist between sessions in `<runtime_dir>/presets.json` (normally `%LOCALAPPDATA%\picalum-qwenimage\presets.json`). Back up that file if you want to keep your preset collection. These presets are managed in the desktop app; they are distinct from the CLI's `--preset`, which selects image quality.

### Surprise: generate an idea for me

Choose **Surprise**, optionally enter a **Theme**, choose quality, and press **Surprise me**. The app constructs an English prompt locally from curated ideas or combinations of subject, style, place and light; it does not call an online prompt-writing service. Choose **Random** format to use the idea's aspect ratio, or select one of the seven fixed ratios.

The generated prompt appears under **Last idea**. **Edit as text** copies that prompt into Create so you can refine it. In a series, Surprise invents a fresh idea for each image, even when the seed is locked.

### Generate several images and stop a run

The main generation button produces one image. Its adjacent arrow opens **Generate 4 images**, **Generate 8 images** and **Generate forever**. These work in Create, regular Edit and Surprise; the area editor runs one masked edit at a time.

- Images are generated **one after another**, saved individually and added to history as they finish. Create and Edit keep the starting prompt, references and settings for the series; Surprise generates a new prompt for each image.
- Without a locked seed, each image receives a random seed. With a locked seed, every request uses the same number, so a series with unchanged inputs is not a way to obtain different compositions.
- **Stop after this image**, available during a finite 4/8-image series, finishes and saves the current image, then starts no further images.
- **Cancel now** or **Esc** requests immediate cancellation of the current generation and prevents the rest of the series from starting. Already completed images remain on disk and in history.
- Finite series have a safety time limit and may finish early on timeout or error. **Generate forever** has no overall series deadline; cancel it when you have enough images.

While a run is active, prompt, reference, quality, seed and mode controls are locked. Progress shows model loading, prompt encoding, sampling steps, decoding and saving; sampling also shows seconds per step and estimated time remaining. There is no live image preview during sampling.

### Review results, compare edits and reuse an image

Click a history thumbnail to select a completed image. Beneath the result, the app shows its prompt, dimensions, seed, quality and generation time. The actions are:

| Action | What it does |
|---|---|
| Copy prompt | Copy the selected image's prompt to the clipboard |
| Use seed | Fill the seed field and enable Lock; it does not restore all other generation settings |
| Edit this image | Switch to Edit and put the selected result first in the reference list, retaining other references up to the 10-photo limit |
| Show in folder | Reveal the selected PNG in Explorer |
| Open | Open it with the system's associated image application |
| Open folder (header) | Open the configured output directory |

Click the result image for an enlarged overlay; click again or press Esc to close it.

For a **regular Edit result** with a recorded first reference, the viewer offers **Result**, **Side by side** and **Wipe**. Side by side displays the source and result together. Wipe overlays them with a draggable divider; focus the divider and use Left/Right arrows to move it, or Shift+Left/Right for larger steps. Use **Fit** or **100%**, or the mouse wheel to switch between them. At 100%, drag an image to pan both views together. Dimensions are shown; differently sized images are displayed without cropping. Keep the source reference available on disk for comparison. The comparison toolbar is currently for regular Edit results, not Create, Surprise or masked edits.

History is **session-only** and the strip displays up to 60 recent results. **Clear history** clears the preview and in-app list, not the saved PNGs or JSONs. Restarting the app does not reload a gallery of earlier sessions; use **Open folder** to access those files.

### Output settings, metadata and local files

Open the gear button → **Output settings** to choose an output folder and enable / disable **Create JSON metadata files**, then press **Save**. Cancel closes the dialog without applying changes. Settings cannot be changed during generation. The chosen folder is shared with the CLI; changing it does not move existing images.

- Images are saved as PNG, normally in `%LOCALAPPDATA%\picalum-qwenimage\ComfyUI\output`, unless you choose another directory. Folder-mode filenames include a timestamp, generation mode and seed; existing image files are not overwritten.
- Prompt, model and generation parameters are embedded in each PNG even with JSON saving off. Optional adjacent JSON files contain additional details such as reference paths and timings. JSON saving is enabled by default on a fresh configuration. Remember that sharing a generated PNG also shares its embedded prompt metadata.
- On Windows, the shared configuration is normally `%APPDATA%\picalum-qwenimage\config.json`. The engine, models (unless relocated), prompt presets and engine log live under the runtime folder, normally `%LOCALAPPDATA%\picalum-qwenimage`.
- Installation needs network access to download dependencies and weights. Installed generation and prompt-idea creation run locally; reference uploads go to the local engine on `127.0.0.1`, not a cloud image service.

### Keyboard shortcuts

| Context | Shortcut | Action |
|---|---|---|
| Main workspace | Ctrl+Enter | Generate one image with the current settings |
| Main workspace | Esc | Close the enlarged image, or cancel the active generation when no enlarged image is open |
| Area editor | Ctrl+Enter | Generate the masked edit |
| Area editor | Ctrl+Z / Ctrl+Y or Ctrl+Shift+Z | Undo / redo mask strokes when focus is not in a text field |
| Area editor | Mouse wheel | Zoom around the pointer |
| Area editor | Space+drag or middle-mouse drag | Pan the photo |
| Wipe divider | Left / Right; Shift+Left / Right | Move the before/after divider in small / large increments |
| Settings / prompt manager | Esc | Close without saving |

## Mask-guided editing

In **Edit**, **Edit an area** opens the first photo in a brush editor (brush, eraser, size, edge softness, undo/redo, zoom and pan). Describe what the painted area should become; optionally choose another photo as a reference and call it `<image2>`.

1. Add the canvas photo in Edit and choose quality / seed before opening **Edit an area**. Add any optional content-reference photo to the main reference list too.
2. Paint the area with **Brush**; use **Eraser** to subtract from it and **Size** to adjust the brush diameter. **Undo**, **Redo** and **Clear** change the selection; **Fit** resets the view. The red overlay is the selected area, not the final result.
3. Adjust **Edge softness** for the blend at the inside of the mask boundary. Use zero for a hard edge or a larger value for a softer transition.
4. Write a specific instruction. If other references were added, choose one under **Reference photo**; within this edit it is `<image2>`, regardless of its position in the main list. Choose **None** to use only the instruction.
5. Press **Generate**. On success, the dialog closes and the final composite is saved and shown in history. A failed or cancelled generation keeps the selection in the open dialog so you can retry. Closing a non-empty painted selection asks whether to discard it; during generation, the close button / Esc requests cancellation.

How it works, and what it does and does not guarantee:

- The model never receives a mask. It sees your photo with the area marked by a red outline and tint, and is told to edit only inside it. The prototype in `bench/mask_proto` compared this with sending the mask as a second image (the mask outline leaked into results) and with the instruction alone (it over-edited neighbouring objects).
- The app then composites the result: **outside the painted area the final image has exactly the source pixels**. Inside it, the edge is softened inward by the "edge softness" setting; the painted area is never expanded.
- The model may draw outside the painted area; that part is discarded. When placing an object from a reference photo, paint an area big enough for the whole object, or it will be cut off.
- Be specific. "Dark smoky eye shadow and bold red lipstick" works; "heavy makeup" gets a timid result. Fine edits come out better on Standard than on Draft.
- The result has the source size (resampled from the engine's canvas) and is the only file added to the history; the source file is never modified. The mask is not saved: re-running an edit means painting it again.
- Experimental: tested on a few photos and seeds on an RTX 3060, `balanced` profile; not yet on 8 GB or the compact profile.

## Honest benchmark: where it wins and where it loses

20 prompts, **one run per model, no cherry-picking**. Qwen-Image-2.1 on an RTX 3060 12 GB (int8, 40 steps, native 2K) against gpt-image-2 in the cloud. Edits use real photos from Wikimedia Commons.

| Category | Qwen wins | Tie | gpt-image-2 wins |
|---|---:|---:|---:|
| Photography (6) | 0 | 1 | 5 |
| Creativity (6) | 0 | 0 | 6 |
| **Photo editing (8)** | **1** | **7** | **0** |
| **Total** | **1** | **8** | **11** |

**For editing photos it is on par with gpt-image-2**, free, private and unlimited. **For text-to-image it is not**: it follows long prompts less closely and knows less about the world (asked for a *mate*, it drew a Chinese gourd twice).

## How it fits in 12 GB

The three models don't fit together: text encoder 8.9 GB + image model 6.9 GB + VAE 0.6 GB = 16.4 GB. So they run **one stage at a time, each loaded completely into VRAM**, while the others wait in system RAM. Nothing is split between CPU and GPU, which is what makes low-VRAM setups slow. Verified in the engine log (`loaded completely … full load: True` for all three stages) and by the speed: 1.35 s per step at 1024², which is compute-bound on an RTX 3060.

Two choices matter more than anything else:

- **int8 "convrot" weights, not GGUF.** Ampere and newer cards have int8 tensor cores, and ComfyUI ships native kernels for this format, so it is faster than dequantizing GGUF on the fly and close to lossless.
- **PyTorch built for CUDA 13.** With CUDA 12.8 builds, ComfyUI silently disables its optimized CUDA backend. The installer picks the right build and checks your driver (R580 or newer).

## Install

**Requirements:** Windows 10/11 · NVIDIA GPU, **tested only on RTX 3060 12 GB** · driver 580+ · ~25 GB free disk · 32 GB RAM (what it was tested with; 16 GB is untested). Other GPU models have not been tested. Real 10–<12 GB cards are untested; Compact is experimental and successful generation is not guaranteed. See [GPU support and verification](#gpu-support-and-verification).

### Option A · Build it yourself (recommended, ~5 min)

A binary you compile on your own PC carries no "downloaded from the internet" mark, so Windows SmartScreen won't flag it, and you know exactly what you are running.

**1. Prerequisites, once.** Paste into any terminal:

```powershell
winget install Rustlang.Rustup
winget install OpenJS.NodeJS.LTS
winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Close and reopen the terminal afterwards so it picks up the new tools.

**2. Clone and build.** Pick the block for the terminal you use:

<table>
<tr><th>Terminal</th><th>Command</th></tr>
<tr><td><b>PowerShell 5.1</b><br/><sub>the one that ships with Windows</sub></td><td>

```powershell
git clone https://github.com/thejohnd0e/PicaLum-QwenImage; cd PicaLum-QwenImage
powershell -ExecutionPolicy Bypass -File ./build.ps1
```

</td></tr>
<tr><td><b>PowerShell 7</b><br/><sub>pwsh, Windows Terminal default</sub></td><td>

```powershell
git clone https://github.com/thejohnd0e/PicaLum-QwenImage; cd PicaLum-QwenImage
pwsh -ExecutionPolicy Bypass -File ./build.ps1
```

</td></tr>
<tr><td><b>CMD</b><br/><sub>Command Prompt</sub></td><td>

```bat
git clone https://github.com/thejohnd0e/PicaLum-QwenImage && cd PicaLum-QwenImage
.\build.cmd
```

</td></tr>
<tr><td><b>Git Bash · Warp</b><br/><sub>any bash on Windows</sub></td><td>

```bash
git clone https://github.com/thejohnd0e/PicaLum-QwenImage && cd PicaLum-QwenImage
./build.sh
```

</td></tr>
<tr><td><b>No terminal</b></td><td>

Download the [source zip](https://github.com/thejohnd0e/PicaLum-QwenImage/archive/refs/heads/master.zip), extract it and **double-click `build.cmd`**.

</td></tr>
</table>

All five run the same `build.ps1`, which checks the prerequisites (and tells you the exact command for anything missing), then leaves `picalum-qwenimage.exe` and `qil.exe` in `out\`. Add `-Installer` to also build the setup `.exe`. Tested with PowerShell 5.1, PowerShell 7.6, CMD and Git Bash.

**3. Run** `out\picalum-qwenimage.exe`. The first launch installs the engine and downloads about **19.5 GB** once: engine, Python, PyTorch, and the weights from [Hugging Face](https://huggingface.co/Comfy-Org/Qwen-Image-2.1). Downloads resume if interrupted and are checked against SHA-256.

### Option B · Prebuilt installer

Download `PicaLum QwenImage_x.y.z_x64-setup.exe` from [Releases](https://github.com/thejohnd0e/PicaLum-QwenImage/releases/latest) and run it; it installs for your user only, no admin. It is not code-signed (certificates are paid), so SmartScreen will warn the first time: **More info → Run anyway**.

The downloaded runtime normally lives in `%LOCALAPPDATA%\picalum-qwenimage`; models and generated images can be stored on another disk. To remove an installed desktop app, use Windows **Installed apps**. To remove downloaded models and the runtime too, delete the runtime folder after closing the app. Configuration in `%APPDATA%\picalum-qwenimage` and any separately chosen model / output folders are separate; keep your generated images and preset backups if you want to reuse them.

## GPU support and verification

Windows NVIDIA is required. **We tested only an RTX 3060 with 12 GB VRAM. Other GPU models and real 10–<12 GB cards have not been tested. Compact on 10–<12 GB cards is experimental: installation eligibility does not guarantee that generation will succeed or fit in VRAM.** The software's minimum eligibility threshold is 10,000 MB total VRAM; 10,000–11,999 MB selects Compact and 12,000 MB or more selects Balanced. These thresholds are selection rules, not test results for every eligible card. `qil setup --profile ...` is a manual profile choice and never overrides the 10 GB floor or unsupported-hardware checks. On multi-GPU systems, pass `--gpu-uuid` with the stable UUID shown by `qil doctor`; the CLI does not silently choose a different card.

Setup proves the selected GPU's architecture with the bundled PyTorch before any model weights are downloaded. The desktop app does not run a test generation on its own; Settings → Check GPU generates one Draft image on demand for the selected GPU/profile/runtime. Cancel waits for cancellation to be confirmed and persisted. `qil doctor` reports all detected GPUs, the selected UUID and profile recommendation, transient free VRAM, and whether the stored verification is fresh. Verification confirms this installed runtime context only; it is not a benchmark and does not prove performance or future driver compatibility.

## Quick start (CLI)

`qil.exe` comes out of the same build (or the release zip) and uses the same install as the app.

Start with `qil doctor` to inspect installation and GPU selection. It reports GPU inventory / UUIDs, total and transient free VRAM, architecture support, the configured and recommended profiles, verification freshness, paths, free disk and missing setup steps. Its **Ready** message describes installation readiness; it is not itself a successful test generation. Use `qil --help` or `qil <command> --help` for the command syntax.

Abbreviated example (your GPU, UUID, paths and verification state will differ):

```text
> qil doctor
GPU status NVIDIA GPU detected
profile    balanced (12 GB+ (int8 encoder))
NVIDIA GeForce RTX 3060 · UUID GPU-… · total 12288 MB · used 1024 MB · free 11264 MB transient · supported: 12 GB+ (int8 encoder)
capability proven 8.6
selected   UUID GPU-…
automatic  12 GB+ (int8 encoder)
verification none
  [x] uv 0.12.18 (Python manager)
  [x] ComfyUI engine
  [x] Python 3.12
  [x] PyTorch (CUDA 13) + engine packages
  [x] qwen_image_2.1_int8_convrot.safetensors (6.76 GB)
  [x] qwen3vl_8b_int8_convrot.safetensors (8.71 GB)
  [x] qwen_image_2.1_vae_bf16.safetensors (644 MB)
Ready. Try: qil generate "a red fox asleep in the snow" --preset draft
```

```bash
qil setup                                             # install; profile picked from your VRAM
qil generate "a red fox asleep in the snow" --preset draft
qil generate "a mate gourd sticker" --transparent --aspect 1:1
qil edit "Change the scene to a snowy night" -i street.jpg --preset standard
qil edit "Put the dog from <image2> next to the bike in <image1>" -i bike.jpg -i dog.jpg
qil surprise --theme capybaras
qil bench --suite bench/suite.json --preset max       # reproduce the published benchmark
```

Pass `--keep-engine` to leave the engine running between calls so each one skips the start-up.

Generated images are saved by default to the bundled runtime's `ComfyUI/output` folder. Change the output folder in desktop settings or with `qil config --output-dir`; the CLI also accepts `--out` for a one-command destination. The desktop app can create an adjacent JSON metadata file, while prompt and model metadata are always embedded in each PNG. History is kept only for the current app session: **Clear history** and app restart reset the in-app list without deleting saved files.

Edit-mode prompt presets store an editable name and prompt text locally at `<runtime_dir>/presets.json`. The default location is `%LOCALAPPDATA%\picalum-qwenimage\presets.json`; presets do not change quality, format, seed, or reference-photo settings.

### CLI command and option reference

| Command | Options and behavior |
|---|---|
| `qil doctor` | Inspect GPU / installation / verification status; does not install or generate |
| `qil setup` | `--profile balanced\|compact` chooses manually; omitting it chooses from VRAM. `--gpu-uuid GPU-…` selects a stable GPU identity. `--models-dir PATH` reuses / relocates weights; `--runtime-dir PATH` chooses the runtime directory. Setup saves the configuration |
| `qil generate "PROMPT"` | Create one image; accepts the shared options below and `--transparent` for native RGBA |
| `qil edit "INSTRUCTION" -i PHOTO` | Edit one or combine up to 10 references; repeat `-i` / `--image` for each file, in order. The first reference is the canvas. Accepts the shared options below |
| `qil surprise` | Create one locally invented idea; optional `--theme "THEME"`. Accepts shared quality, seed and output options; this command uses the idea's generated aspect ratio rather than `--aspect` |
| `qil config` | Show shared configuration as JSON. `--profile balanced\|compact`, `--models-dir PATH` and `--output-dir PATH` update the saved settings; changing paths does not move old files or download models |
| `qil bench` | Run a JSON suite and record images, timing, sampling speed, dimensions, seed, profile, GPU and peak VRAM in `results.json`; options below |

Shared options for `generate`, `edit` and `surprise`:

- `-p, --preset draft|standard|max`: quality, default **standard**. This selects steps / resolution, not a saved editing prompt.
- `-a, --aspect RATIO`: one of the seven listed ratios, default `1:1`; used for text creation. Editing takes its canvas from the first photo; Surprise uses the generated idea's ratio.
- `--seed NUMBER`: use a fixed seed; omitted means random.
- `-o, --out PATH`: a destination folder or a `.png` filename for this run; omitted uses the configured folder. Existing files receive a unique name instead of being overwritten.
- `--keep-engine`: leave the started engine running for subsequent calls. Without this flag, a CLI-owned engine stops when the command exits; an externally started engine is reused and not stopped by the CLI.

For `bench`, defaults are `--suite bench/suite.json`, `--out bench/results/qwen`, `--preset max` and `--seed 20260923`. `--inputs PATH` overrides the input-photo folder (otherwise `<suite folder>/inputs`). Repeat `--only ID` to select tests: explicitly selected tests run even if they already have results. Without `--only`, completed tests are skipped unless `--force` is set. `--keep-engine` leaves the engine running after the suite. Each test's result or error is written to `results.json`, so a printed results path alone does not mean every test succeeded.

```powershell
qil config --output-dir "D:\Qwen Images"
qil generate "a red fox asleep in the snow" --preset standard --aspect 16:9 --seed 42 --out "D:\Qwen Images\fox.png"
qil edit "Place the subject from <image2> into <image1>" -i scene.jpg -i subject.png --seed 42
qil bench --only E3_cartel_texto --only E7_bici_recorte --preset max
```

Desktop batches, the area-mask editor, the comparison viewer and prompt-preset management are desktop controls; the CLI currently generates one image per `generate` / `edit` / `surprise` invocation and does not expose a mask-edit command.

## Troubleshooting

| Symptom | What to check |
|---|---|
| Setup is blocked | Read the reported reason; Windows NVIDIA, at least 10,000 MB total VRAM, a sufficiently recent driver, compatible bundled-PyTorch architecture and enough free disk are required to pass setup eligibility. 10–<12 GB remains experimental, with no guarantee of generation. Selecting a manual profile does not bypass these checks |
| Download or installation failed | Expand setup **Details**, correct the network / disk / reported package problem, then retry. Keep partial downloads if you want them to resume |
| Engine does not start | Read the displayed error and `<runtime_dir>/engine.log` (normally `%LOCALAPPDATA%\picalum-qwenimage\engine.log`); run `qil doctor` to check missing runtime steps and selected GPU |
| GPU out-of-memory error | Close other GPU-heavy applications and check the VRAM meter. Total VRAM eligibility is not a guarantee that enough VRAM is currently free. Compact is the experimental lower-memory profile; it needs its own encoder weights |
| Generate is disabled | Wait for Engine ready, enter a prompt / instruction, and in Edit add at least one reference. Generation and GPU verification cannot run together |
| Check GPU failed | Read the verification error and retry from the verification card or Output settings. This optional test does not make the app unusable; it checks the installed GPU / profile / runtime context |
| Older images are missing from history | History resets on restart. Open the output folder; clearing history does not delete images. After changing the output path, older files remain in the previous folder |
| Prompt presets cannot load or save | Use the shown error and Retry loading. Presets need non-empty names and prompts; check that the runtime folder is writable and preserve a backup before manually editing `presets.json` |
| Edit changes too much | Say what must remain unchanged, or use Edit an area to preserve every pixel outside the painted mask. Make the selected area large enough for any inserted object |

## VRAM profiles

All measurements below were taken on the **RTX 3060 12 GB**, including the Compact experiment. They are not measurements on a real 10 GB or 8 GB card.

| Profile | Text encoder | Peak VRAM (desktop included) | 1024², 20 steps | Status |
|---|---|---:|---:|---|
| **Balanced** | int8, 8.9 GB | 11.4 GB | 32 s | Tested on RTX 3060 12 GB |
| **Compact** | w4a8, 6.3 GB | 8.9 GB | 33 s | Measured on RTX 3060 12 GB; real 10 GB cards untested, no guarantee |

The compact profile cuts peak VRAM by about 2.5 GB in the measured RTX 3060 12 GB setup. **Real 10–<12 GB cards have not been tested; successful generation is not guaranteed. 8 GB cards are below the current 10 GB eligibility floor and setup rejects them.** Historical 8 GB experiments in [bench/tiers](bench/tiers) used ComfyUI's soft `--reserve-vram` cap on the same RTX 3060 12 GB, and peak usage still went above 8 GB; they do not establish support for a real 8 GB GPU. Experimental measurements on a real 10 GB card would be especially useful.

## What it doesn't do (yet)

- **NVIDIA on Windows; tested only on RTX 3060 12 GB.** Other GPU models are untested. Real 10–<12 GB cards are untested and experimental, with no guarantee of successful generation. AMD, Intel, Apple Silicon and GPUs below the 10 GB eligibility floor are not supported by this app. For a different backend, see [nihui/qwenimage-ncnn-vulkan](https://github.com/nihui/qwenimage-ncnn-vulkan), a separate CLI project that trades speed for memory.
- **No live preview** while sampling; you see the final image.
- **Masks are applied by the app, not by the model.** Qwen-Image-2.1 has no mask input, so there is no native inpainting: see [Mask-guided editing](#mask-guided-editing). Same-size output only; outpainting and custom canvas sizes are not available yet.
- **No prompt enhancer.** Qwen's official rewriter is a separate 9B model; it doesn't fit alongside on 12 GB.
- **Not for commercial use** while the weights stay under the Qwen Research License.

## FAQ

**What AI image model runs on a 12 GB GPU like an RTX 3060?** Qwen-Image-2.1 with int8 weights, loading one stage at a time. This app does that setup for you: about 35 s per 1024² image, about 7 minutes at native 2K.

**Is Qwen-Image-2.1 as good as gpt-image-2?** For editing photos, yes in our tests (7 ties and 1 win out of 8). For text-to-image, no (gpt-image-2 won 11 of 12).

**Can I use my existing ComfyUI models folder?** Yes. Choose it on the setup screen or run `qil setup --models-dir <folder>`. Files that match in size are not downloaded again.

## Repo layout

```
crates/qil-core   installer, engine lifecycle, generation client, GPU detection (Rust)
crates/qil-cli    the qil command line
app/              Tauri 2 desktop app (Rust + plain HTML/CSS/JS, English, Russian and Spanish)
bench/            suite.json, input photos with credits, results, verdicts, site builder, mask-guidance prototype
docs/             GitHub Pages benchmark site
```

`build.ps1` / `build.cmd` / `build.sh` wrap the two builds: `cargo build --release -p qil-cli` and `npx tauri build` in `app/`.

## Help it spread

This exists because a 12 GB card deserved better than "buy a 4090". If it ran on yours:

- **⭐ [Star the repo](https://github.com/thejohnd0e/PicaLum-QwenImage/stargazers).** It is the single thing that makes GitHub and search engines show it to the next person with a small GPU.
- **Share it** where people ask "what can I run on my card?": [r/StableDiffusion](https://www.reddit.com/r/StableDiffusion/), [r/LocalLLaMA](https://www.reddit.com/r/LocalLLaMA/), [X / Twitter](https://twitter.com/intent/tweet?text=Qwen-Image-2.1%20running%20fully%20in%20VRAM%20on%20a%2012%20GB%20GPU%2C%20desktop%20app%20%2B%20honest%20benchmark%20vs%20gpt-image-2&url=https%3A%2F%2Fgithub.com%2Fthejohnd0e%2FPicaLum-QwenImage), your Discord.
- **Post your numbers.** Own a 12 GB or larger NVIDIA card, or want to test an experimental 10 GB Compact configuration? Run `bash bench/tiers/run.sh` and open an issue with `results.json`. Real 10 GB results are especially useful, but successful generation is not guaranteed; the current app does not support 8 GB cards.

## Credits

[Qwen-Image-2.1](https://github.com/QwenLM/Qwen-Image-2.1) by the Qwen team at Alibaba · [ComfyUI](https://github.com/comfyanonymous/ComfyUI) and the [Comfy-Org](https://huggingface.co/Comfy-Org/Qwen-Image-2.1) int8 repacks · [uv](https://github.com/astral-sh/uv) · [Tauri](https://tauri.app). Benchmark photos from Wikimedia Commons; authors and licenses in [bench/inputs/credits.json](bench/inputs/credits.json).

## License

The code in this repo is [MIT](LICENSE). The Qwen-Image-2.1 weights, which the app downloads from Hugging Face, are under the [Qwen Research License](https://huggingface.co/Qwen/Qwen-Image-2.1/blob/main/LICENSE) and are **not licensed for commercial use**. ComfyUI is GPL-3.0 and runs as a separate process. Edits of CC BY-SA benchmark photos are CC BY-SA 4.0.
