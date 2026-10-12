<div align="center">

# PicaLum QwenImage

**Create pictures from a description, edit your photos, or combine several photos — on your own Windows PC.**

Your photos and prompts stay on your computer. No cloud image service or subscription is needed.

[![Status](https://img.shields.io/badge/status-beta-d29922?style=flat-square)](CHANGELOG.md)
[![Windows](https://img.shields.io/badge/windows-10%20%7C%2011-0078D4?style=flat-square&logo=windows&logoColor=white)](#what-you-need)
[![Tested GPU](https://img.shields.io/badge/tested%20GPU-RTX%203060%2012%20GB-76B900?style=flat-square&logo=nvidia&logoColor=white)](#what-you-need)
[![Model](https://img.shields.io/badge/model-Qwen--Image--2.1-615CED?style=flat-square)](https://huggingface.co/Qwen/Qwen-Image-2.1)
[![Code license](https://img.shields.io/badge/code%20license-MIT-green?style=flat-square)](LICENSE)

<p><img src="assets/Screenshot%20-%202026-10-12%2017.44.28.png" alt="PicaLum QwenImage — Create mode with a description field and image settings" width="100%"/></p>
<p><img src="assets/Screenshot%20-%202026-10-12%2017.44.39.png" alt="PicaLum QwenImage — Edit mode with photo references and editing instructions" width="100%"/></p>

[**Download Windows installer**](https://github.com/thejohnd0e/PicaLum-QwenImage/releases/latest) · [Get started](#get-started) · [All features](#what-you-can-do) · [Help](#troubleshooting) · [For developers](#for-developers-and-advanced-users)

</div>

## What you can do

| Feature | In plain words |
|---|---|
| **Create pictures** | Describe what you want: a portrait, landscape, illustration, poster or another idea |
| **Edit a photo** | Ask to change the lighting, style, background, clothing or text, or remove objects or people |
| **Combine photos** | Use up to **10 photos** together — for example, put a subject from one photo into the scene of another |
| **Change just one area** | Paint the area to edit and describe the change. Everything outside it stays unchanged. You can also choose another photo as the content for that area. This feature is beta |
| **Create with a transparent background** | Make a new image with no background, useful for stickers and cutouts |
| **Let the app invent an idea** | Use **Surprise**, with an optional theme, then keep or edit the proposed description |
| **Choose quality and shape** | Three quality levels: Draft, Standard and Max. Choose square, portrait or landscape shapes from seven proportions |
| **Make several versions** | Generate one image, a series of **4 or 8**, or keep generating until you stop |
| **Control when to stop** | Cancel the current generation, or let a finite series finish its current image before stopping |
| **Reuse a random starting number** | Lock the **seed** to reuse the same starting number, or leave it random for new variations |
| **Save editing instructions** | Use six ready-made prompts, or create, duplicate, rename, edit and delete your own |
| **Prepare your photos** | Add photos by drag-and-drop or a file picker, rotate them, and remove references without changing the original files |
| **Compare before and after** | For regular photo edits, view the original and result side by side or move a comparison slider; zoom and pan to inspect details |
| **Review and reuse results** | Select recent images, enlarge them, copy their descriptions, reuse a seed or send a result back for another edit |
| **Save automatically** | Every completed image is saved as a PNG. Choose the output folder, open images in another app or reveal them in Explorer |
| **Keep generation details** | Descriptions and settings are stored inside the PNG; separate JSON information files are optional |
| **See progress** | Follow generation steps, the estimated remaining time and graphics-memory usage |
| **Manage the engine** | Start or stop the program's image engine, and run an optional **Check GPU** test from Settings |
| **Use your language** | English, Russian and Spanish interface; your choice is remembered |
| **Use keyboard shortcuts** | Generate with Ctrl+Enter, cancel with Esc, and use shortcuts in the area editor |
| **Work locally** | The app downloads what it needs once. Image generation and the idea generator then run on your PC |
| **Automate from the terminal** | An optional command-line tool supports generation, editing, setup, diagnostics and benchmarks; see the [advanced section](#quick-start-cli) |

The app uses Qwen-Image-2.1. Results depend on the model and your instructions: an editing prompt is a request, not a guaranteed photo filter. **The model's license allows non-commercial use only.** This is an unofficial app, not affiliated with Alibaba or the Qwen team.

## What you need

- **Windows 10 or 11** and an **NVIDIA graphics card**.
- **Tested only on RTX 3060 with 12 GB graphics memory.** Other GPU models have not been tested. Real 10 GB cards have not been tested either; the lower-memory Compact option is experimental and may not work. Cards below 10 GB are rejected by setup.
- NVIDIA driver **580 or newer**.
- About **25 GB of free disk space** for the initial installation, plus space for your images.
- **32 GB RAM** is the tested system configuration; 16 GB has not been tested.
- Internet for the first download. Your descriptions and photos are not sent to a cloud image service.

A Draft image takes about **35 seconds on the tested RTX 3060 12 GB**. Larger images and photo edits take longer; the first run also needs time to load the models. Other computers may behave differently.

## Get started

1. **[Download the Windows installer](https://github.com/thejohnd0e/PicaLum-QwenImage/releases/latest).** Choose the file whose name ends in **`x64-setup.exe`**, not the source-code archives. You do not need to build anything or install development tools.
2. **Run the installer**, then open **PicaLum QwenImage**. It installs for your Windows user without administrator rights. The installer is not code-signed; if Windows SmartScreen appears, choose **More info → Run anyway**.
3. **Complete the app's first setup.** Keep the recommended profile for the tested RTX 3060 12 GB. If several graphics cards are listed, select one. The default models folder is fine; **Models folder → Change…** lets you use another disk or reuse existing model files.
4. **Press Install in the app.** It downloads its image engine and model files — about **19.5 GB** for the default installation. This is a one-time download; progress is shown and interrupted downloads can resume when you retry.
5. **Wait for Engine ready.** Choose **Create**, write a description such as `A red fox asleep in the snow`, and press **Generate**.
6. **Open your result folder.** Completed images are saved automatically. The **Open folder** button takes you to them.

You do not need Rust, Node.js, Visual Studio, ComfyUI experience or manual Python-package setup. After the first setup, opening the app takes you straight to the workspace. [Building from source](#build-from-source) is for developers and people who specifically want to compile the application themselves.

## Desktop user guide

Button names below are in English; the same controls are available in Russian and Spanish.

### Create an image

1. Choose **Create** and describe the subject, setting, style and lighting you want.
2. Choose **Draft** to try an idea quickly, **Standard** for the default quality, or **Max** for a larger, slower result.
3. Choose **Format**: square (`1:1`), landscape (`4:3`, `3:2`, `16:9`) or portrait (`3:4`, `2:3`, `9:16`).
4. For a new image without a background, enable **Transparent background**. This option applies to Create; to change an existing photo's background, use Edit.
5. Leave **Seed → Lock** off for a new random starting number. Turn it on and enter a number if you want to reuse it. A seed alone does not preserve an image when you change the prompt, quality or other inputs.
6. Press **Generate** or **Ctrl+Enter**. The image is saved automatically when it finishes.

| Quality | Best for | Approx. Create / Edit time on RTX 3060 12 GB |
|---|---|---|
| Draft | Quick experiments; about 1 megapixel for new images | 35 s / 50 s |
| Standard | Everyday use; about 1.6 megapixels for new images | 1.5 min / 2.5 min |
| Max | Larger new images, up to the model's native 2K presets | 7 min / 5 min |

These are estimates for the tested computer. A regular photo edit follows the first photo's framing, but its saved size can differ from the original. **Edit an area** keeps the original size.

### Edit and combine photos

1. Choose **Edit**. Click the reference area / **+**, or drag photos onto the app. Dropping files also switches to Edit. You can add up to **10** PNG, JPG/JPEG, WebP or BMP files.
2. Add the photo you want to change **first**. It is the canvas. Additional photos can provide subjects, styles or other reference material.
3. Write your instruction. Refer to the numbered photos as `<image1>`, `<image2>`, and so on. For example: `Place the dog from <image2> next to the bicycle in <image1>, matching lighting and perspective.`
4. Use **↻** on a thumbnail to rotate it 90° clockwise, or **×** to remove it from the list. Rotation uses a copy; the original stays unchanged. To change the canvas, remove / re-add photos in the desired order.
5. Choose quality, optionally lock a seed, and press **Edit photo** / **Generate with … photos**.

Tell the model both **what to change** and **what to keep**. Ordinary editing can change details outside your intended edit; use the area editor when those pixels must stay unchanged.

### Change only a selected area

1. In Edit, add the photo you want to change first. Choose quality and seed before opening **Edit an area**. Add any optional content-reference photo to the reference list too.
2. Click **Edit an area**. Paint the part to change with **Brush**. The red overlay shows your selection.
3. Use **Eraser** to subtract from it, **Size** to adjust the brush, and **Undo**, **Redo** or **Clear** to correct the selection. Use the mouse wheel to zoom; hold Space or the middle mouse button and drag to move the photo. **Fit** resets the view.
4. Set **Edge softness**: zero gives a hard boundary; a larger value blends the result more gently inside the selected edge.
5. Describe exactly what should appear there. If you added other photos, select one under **Reference photo**; call it `<image2>` in this instruction, regardless of its position in the main list. Choose **None** to use the instruction only.
6. Press **Generate**. The saved result keeps the source size, and **every pixel outside the selected area stays unchanged**. The original file is never modified.

Paint enough space for the entire inserted object: anything the model draws outside the selection is discarded. Specific instructions work better, for example `Dark smoky eye shadow and bold red lipstick`. Standard is a better starting point for fine edits than Draft.

On success, the editor closes and only the final result is added to history. If generation fails or is cancelled, the selection stays in the open editor so you can retry. Closing a painted selection asks whether to discard it. During generation, the close button / Esc requests cancellation. The selection is not saved for another session or after the editor is closed.

This feature is **beta**, tested on a few photos and seeds on RTX 3060 12 GB with Balanced. It has not been tested with Compact. See [how area editing works](#mask-guided-editing) for the technical explanation.

### Save and manage editing prompts

In Edit, **Choose a prompt preset…** fills the instruction field with a saved prompt. You can change its text before generating; selecting it does not start generation or change photos, quality or seed.

The six initial prompts are **Remove background**, **Snowy night**, **Watercolor**, **Golden hour**, **Remove people** and **Combine 1 + 2**. They are editable instructions for the model, not separate guaranteed effects.

Open **Manage presets** to:

- **New**: add your own named prompt.
- **From current prompt**: save the instruction already in the Edit field as a new preset.
- **Duplicate**: copy the selected preset.
- Change its **Name** and **Prompt**, or **Delete** it.
- Press **Save** to keep all changes. Every preset needs a name and prompt. **Cancel**, Esc or closing without saving discards the changes.

Saved prompts remain available after you restart the app. They store the instruction, not image settings.

### Surprise: let the app invent an idea

Choose **Surprise**, optionally enter a **Theme**, select quality and press **Surprise me**. The app invents an English description locally. Choose **Random** format to let it choose the image's shape, or pick one of the seven fixed proportions.

The description appears under **Last idea**. **Edit as text** copies it into Create so you can refine it. A Surprise series invents a fresh idea for each image, even with a locked seed.

### Make a series and stop generation

The main button makes one image. Its arrow opens **Generate 4 images**, **Generate 8 images** and **Generate forever**. These work in Create, regular Edit and Surprise; the area editor makes one result at a time.

- Images are made one after another and saved as they finish. Create / Edit keep the starting inputs and settings; Surprise invents a new idea each time.
- Leave the seed unlocked for random variations. A locked seed is reused for every image; unchanged inputs can repeat the same composition.
- **Stop after this image** finishes the current image and stops the rest of a finite 4/8-image series.
- **Cancel now** or **Esc** requests cancellation of the current run and stops the series. Images already completed remain saved.
- Finite series have a safety time limit and can stop on timeout or error. **Generate forever** has no overall series deadline; cancel it when you have enough images.

Inputs are locked while generation runs. The progress display shows the current stage, sampling steps and estimated time left. You see the completed picture when it is ready; there is no live picture preview during generation.

### View, compare and reuse results

Click a history thumbnail to select a result. Its description, size, seed, quality and generation time appear beneath it.

| Button | What happens |
|---|---|
| Copy prompt | Copy the selected image's description |
| Use seed | Fill the seed field and enable Lock; other settings are not restored |
| Edit this image | Put this result first in Edit, keeping other references up to the 10-photo limit |
| Show in folder | Reveal the saved PNG in Explorer |
| Open | Open it in your usual image application |
| Open folder (header) | Open your selected output folder |

Click the result for an enlarged view; click again or press Esc to close it.

For **regular photo edits**, the viewer has **Result**, **Side by side** and **Wipe**. Side by side displays the original and result together. Wipe lets you drag a divider over them. Use **Fit** or **100%**, or the mouse wheel to switch; at 100%, drag to move both views together. Different-sized images are displayed without cropping. Keep the original photo on disk so comparison can load it. This comparison toolbar is not currently available for Create, Surprise or area-edited results.

**History is only for the current session**, with up to 60 recent thumbnails visible. Restarting the app clears this list. **Clear history** clears the list and preview, not your saved files. Use Open folder to find images from earlier sessions.

### Settings, saving and engine controls

- **Choose a save folder:** gear button → **Output settings** → change the folder → **Save**. Existing pictures remain in their previous folder. Settings cannot be changed during generation; Cancel closes without saving.
- **Save extra information:** **Create JSON metadata files** adds a small information file beside each picture. This is on by default. Even with it off, the PNG contains the description and generation settings; sharing the PNG also shares that information.
- **Check your graphics card:** **Check GPU** in Settings makes one Draft test image. It is optional, never runs automatically on opening the app, and does not guarantee every other generation will succeed. If it fails, read the error and retry; the app remains usable.
- **Free graphics memory:** **Stop engine** stops the app-owned image engine; **Start engine** starts it again. These controls are unavailable during generation. If status says **Engine ready (external)**, another program started it and you need to stop it there.
- **Change language:** the header's language button selects English, Russian or Spanish. English is the initial default; your selection is remembered.
- **Read status:** the header shows engine status, GPU memory usage, app version and a GitHub link.

### Keyboard shortcuts

| Where | Shortcut | Action |
|---|---|---|
| Main workspace | Ctrl+Enter | Generate one image |
| Main workspace | Esc | Close the enlarged view, or cancel generation if no enlarged view is open |
| Area editor | Ctrl+Enter | Generate the area edit |
| Area editor | Ctrl+Z / Ctrl+Y or Ctrl+Shift+Z | Undo / redo painted strokes when not typing in a text field |
| Area editor | Mouse wheel | Zoom around the pointer |
| Area editor | Space+drag or middle-mouse drag | Move the photo |
| Comparison divider | Left / Right; Shift+Left / Right | Move the divider a little / a larger step |
| Settings / prompt manager | Esc | Close without saving |

## Troubleshooting

| Problem | Try this |
|---|---|
| Setup will not start | Read the reason shown. Check your Windows version, NVIDIA driver, graphics card and free disk space. Only RTX 3060 12 GB has been tested; the 10 GB option is experimental |
| Download failed | Check your connection and disk space, expand **Details** to read the error, then retry. Keep partial downloads so they can resume |
| Engine will not start | Read the displayed error. If you need help, include the app version and engine log; its location is in [local files](#local-files-and-metadata) |
| Not enough graphics memory | Close other GPU-heavy apps and check the memory meter. A card with enough total memory can still have too much in use |
| Generate is disabled | Wait for **Engine ready**, enter a description / instruction and, in Edit, add a photo. Generation cannot run during Check GPU |
| Check GPU failed | Read its error, then retry from the test screen or Settings. The test is optional |
| Earlier pictures disappeared | Open the output folder. History resets on restart; your saved pictures are not deleted. If you changed the folder, check the old one too |
| Saved prompts will not load / save | Use the displayed error and **Retry loading**. Every prompt needs a name and instruction. Check that the app can write to its data folder |
| Too much of the photo changes | Say what must stay the same, or use **Edit an area**. Paint enough room for an inserted object |

## Things to know

- This is a **beta** application. Tested hardware is **RTX 3060 12 GB**; other GPU models are untested. Real 10 GB cards are untested and experimental, and cards below 10 GB are not accepted by setup. AMD, Intel and Apple Silicon are not supported.
- The model can make mistakes, especially with detailed text and complicated descriptions. In our comparison it did better at photo editing than at following complex image-creation prompts; [results are below](#honest-benchmark-where-it-wins-and-where-it-loses).
- There is no live image preview while generating, no tool to extend an image beyond its canvas, and no custom size / step controls in the desktop app.
- Your files stay local. Internet is needed to install dependencies and weights, not to send your photos to an online image generator.
- The **model weights are non-commercial**. The app's code is MIT; see [License](#license) for the distinction.

## Help the project

If the app is useful, [star the repository](https://github.com/thejohnd0e/PicaLum-QwenImage/stargazers) or share it with someone who wants to create and edit pictures locally. When reporting a problem, include the app version, GPU model / memory and the displayed error.

---

## For developers and advanced users

The sections below cover source builds, terminal commands, model storage, GPU selection and implementation details. For normal desktop use, the guide above covers the controls.

[![CI](https://img.shields.io/github/actions/workflow/status/thejohnd0e/PicaLum-QwenImage/ci.yml?branch=master&style=flat-square&label=CI)](https://github.com/thejohnd0e/PicaLum-QwenImage/actions/workflows/ci.yml)

### Build from source

**1. Install prerequisites once:**

```powershell
winget install Rustlang.Rustup
winget install OpenJS.NodeJS.LTS
winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Close and reopen the terminal so it picks up the tools. Clone and run the wrapper for your terminal:

**PowerShell 5.1:**

```powershell
git clone https://github.com/thejohnd0e/PicaLum-QwenImage; cd PicaLum-QwenImage
powershell -ExecutionPolicy Bypass -File ./build.ps1
```

**PowerShell 7:**

```powershell
git clone https://github.com/thejohnd0e/PicaLum-QwenImage; cd PicaLum-QwenImage
pwsh -ExecutionPolicy Bypass -File ./build.ps1
```

**CMD:**

```bat
git clone https://github.com/thejohnd0e/PicaLum-QwenImage && cd PicaLum-QwenImage
.\build.cmd
```

**Git Bash / Warp:**

```bash
git clone https://github.com/thejohnd0e/PicaLum-QwenImage && cd PicaLum-QwenImage
./build.sh
```

Alternatively, download the [source zip](https://github.com/thejohnd0e/PicaLum-QwenImage/archive/refs/heads/master.zip), extract it and double-click `build.cmd` after installing the prerequisites.

The wrappers run `build.ps1`, which checks prerequisites, builds the CLI and desktop app, and copies `picalum-qwenimage.exe` and `qil.exe` to `out/`. The underlying builds are `cargo build --release -p qil-cli` and `npx tauri build` from `app/`. Use `-Installer` with `build.ps1` to produce the NSIS setup executable too. The wrappers were tested with PowerShell 5.1, PowerShell 7.6, CMD and Git Bash.

Run `out\picalum-qwenimage.exe` and follow [Get started](#get-started). Building the app and the first-run download of its generation runtime are separate steps.

For development, `cargo test --workspace` runs the Rust workspace tests; inspect the network-dependent download integration test before running it. For the desktop app, run `npm ci` once in `app/`, then `npm run dev` or `npm run build`.

Published installers are listed under [Releases](https://github.com/thejohnd0e/PicaLum-QwenImage/releases). The NSIS installer is per-user and does not require administrator rights. It is not code-signed; Windows SmartScreen may require **More info → Run anyway**.

### Quick start (CLI)

`qil.exe` is built alongside the app and shares its runtime and configuration. Start with `qil doctor` to inspect GPU inventory, selected UUID, profiles, architecture support, verification freshness, paths, disk space and missing setup steps. **Ready** describes installation readiness, not a successful test generation. Use `qil --help` or `qil <command> --help` for syntax.

```powershell
qil setup
qil generate "a red fox asleep in the snow" --preset draft
qil generate "a mate gourd sticker" --transparent --aspect 1:1
qil edit "Change the scene to a snowy night" -i street.jpg --preset standard
qil edit "Put the dog from <image2> next to the bike in <image1>" -i bike.jpg -i dog.jpg
qil surprise --theme capybaras
qil config --output-dir "D:\Qwen Images"
qil generate "a red fox asleep in the snow" --preset standard --aspect 16:9 --seed 42 --out "D:\Qwen Images\fox.png"
qil bench --suite bench/suite.json --preset max
qil bench --only E3_cartel_texto --only E7_bici_recorte --preset max
```

### CLI command and option reference

| Command | Options and behavior |
|---|---|
| `qil doctor` | Inspect GPU, installation and verification status; does not install or generate |
| `qil setup` | `--profile balanced\|compact` selects manually; omission chooses from VRAM. `--gpu-uuid GPU-…` selects a stable GPU identity. `--models-dir PATH` reuses / relocates weights; `--runtime-dir PATH` chooses the runtime directory. Saves configuration |
| `qil generate "PROMPT"` | Create one image; accepts shared options and `--transparent` for native RGBA |
| `qil edit "INSTRUCTION" -i PHOTO` | Edit / combine up to 10 references. Repeat `-i` / `--image` in order; the first is the canvas. Accepts shared options |
| `qil surprise` | Create one local idea, with optional `--theme "THEME"`. Accepts shared quality, seed and output options; uses the idea's generated aspect instead of `--aspect` |
| `qil config` | Show configuration as JSON. `--profile balanced\|compact`, `--models-dir PATH` and `--output-dir PATH` update settings; do not move old files or download models |
| `qil bench` | Run a JSON suite and record images, timing, sampling speed, dimensions, seed, profile, GPU and peak VRAM in `results.json` |

Shared options for `generate`, `edit` and `surprise`:

- `-p, --preset draft|standard|max`: quality, default **standard**; not a saved editing prompt.
- `-a, --aspect RATIO`: one of the seven ratios, default `1:1`, used for text creation. Edit takes its canvas from the first photo; Surprise uses the generated idea's ratio.
- `--seed NUMBER`: fixed seed; omitted means random.
- `-o, --out PATH`: output folder or `.png` filename for this run; otherwise the configured folder. Existing files receive a unique name rather than being overwritten.
- `--keep-engine`: keep a CLI-started engine running for subsequent calls. Otherwise it stops when the command exits. Externally started engines are reused and not stopped by the CLI.

For `bench`, defaults are `--suite bench/suite.json`, `--out bench/results/qwen`, `--preset max` and `--seed 20260923`. `--inputs PATH` overrides the input-photo folder (otherwise `<suite folder>/inputs`). Repeat `--only ID` to select tests; these run even if already recorded. Without `--only`, completed tests are skipped unless `--force` is set. `--keep-engine` leaves the engine running afterwards. Each result or error is recorded, so a printed results path does not mean every test succeeded.

Desktop series, the area editor, comparison viewer and prompt manager are desktop controls; the CLI generates one image per `generate` / `edit` / `surprise` invocation and has no mask-edit command.

### GPU support and verification

**We tested only RTX 3060 12 GB. Other GPU models and real 10 GB cards have not been tested.** Compact for 10–<12 GB is experimental, with no guarantee of successful generation or fitting into VRAM.

The eligibility floor is **10,000 MB total VRAM**. Automatic selection chooses Compact at 10,000–11,999 MB and Balanced at 12,000 MB or more. These are software rules, not test results for every eligible card. Setup offers Auto / Manual and stable GPU selection; manual profiles do not bypass the VRAM floor, driver checks or unsupported architecture checks. On multi-GPU systems, use `qil setup --gpu-uuid` with a UUID from `qil doctor`; the CLI does not silently pick a different card.

The bundled PyTorch proves the selected GPU's architecture before model weights are downloaded. The optional Settings → Check GPU test generates one Draft image for the installed GPU / profile / runtime context. Cancel waits for confirmation and persistence. A fresh verification result is not a general benchmark or a promise about future driver compatibility.

### Quality and resolution details

| Quality | Steps | Text-to-image size | Regular editing resolution setting |
|---|---:|---|---:|
| Draft | 20 | About 1 MP; 1024 × 1024 at `1:1` | 1024 px |
| Standard | 30 | About 1.6 MP | 1280 px |
| Max | 40 | Official native 2K aspect presets; 2048 × 2048 at `1:1` | 1536 px |

Create dimensions depend on aspect ratio. Regular editing gets its canvas from the first reference and can change its pixel dimensions. Area editing resamples and composites back to the source size.

### Local files and metadata

- Default runtime: `%LOCALAPPDATA%\picalum-qwenimage`; default output: its `ComfyUI\output` folder.
- Shared Windows configuration: `%APPDATA%\picalum-qwenimage\config.json`. Desktop output settings also affect the CLI. Changing a path does not move existing files.
- Editing prompts: `<runtime_dir>/presets.json`. Back up this file to preserve your prompt collection; it stores prompt text and names, not quality, references or seed.
- Reused model folders need `diffusion_models/`, `text_encoders/` and `vae/` subfolders. Matching existing weights are reused rather than downloaded again.
- Folder-mode PNG filenames include timestamp, mode and seed. Generation metadata is embedded even when JSON saving is off. Optional JSON sidecars add reference paths, timings and other details; enabled by default on a fresh configuration.
- Engine log: `<runtime_dir>/engine.log`. The installed ComfyUI process listens on `127.0.0.1`; reference uploads are local.
- To remove an installed desktop app, use Windows Installed apps. To remove the downloaded runtime too, close the app and delete its runtime folder. Configuration and separately chosen model / output folders are separate; preserve images and preset backups you want to keep.

### Mask-guided editing

The model does not receive a native mask. It sees the source marked with a red outline and tint, plus an instruction to edit there. The app resamples the model result to the source size and composites it with the source, preserving every pixel outside the painted mask. Edge softness feathers inward; it never expands the edited area. Anything the model draws outside the mask is discarded. Only the final composite enters history; the original is unchanged and the mask is not saved for later sessions.

The prototype in `bench/mask_proto` compared this with sending the mask as another reference (the outline leaked into results) and with instructions alone (neighbouring objects changed). This is application-level compositing, not native Qwen inpainting. The feature was tested on a few photos and seeds on RTX 3060 12 GB, Balanced; Compact is untested for area editing.

### How it fits in 12 GB

The three models do not fit together: text encoder 8.9 GB + image model 6.9 GB + VAE 0.6 GB = 16.4 GB. They run one stage at a time, loaded completely into VRAM while the others wait in system RAM. This was verified in the engine log (`loaded completely … full load: True` for all three stages) and by about 1.35 s per step at 1024² on RTX 3060.

The runtime uses int8 "convrot" weights rather than GGUF, and PyTorch CUDA 13. With CUDA 12.8 builds, ComfyUI disables its optimized CUDA backend in the tested setup. Setup chooses the CUDA 13 build and checks for an R580+ driver. Versions of ComfyUI and installation dependencies are pinned by the app.

### VRAM profiles

All measurements below are from **RTX 3060 12 GB**, including the Compact experiment. They are not measurements on real 10 GB or 8 GB cards.

| Profile | Text encoder | Peak VRAM (desktop included) | 1024², 20 steps | Status |
|---|---|---:|---:|---|
| Balanced | int8, 8.9 GB | 11.4 GB | 32 s | Tested on RTX 3060 12 GB |
| Compact | w4a8, 6.3 GB | 8.9 GB | 33 s | Measured on RTX 3060 12 GB; real 10 GB cards untested, no guarantee |

Compact reduced peak usage by about 2.5 GB in that setup. Historical 8 GB experiments in [bench/tiers](bench/tiers) used ComfyUI's soft `--reserve-vram` cap on the same RTX 3060 12 GB, and peak usage still exceeded 8 GB. They do not establish support for actual 8 GB GPUs; current setup rejects those cards.

### Honest benchmark: where it wins and where it loses

20 prompts, one run per model, no cherry-picking: Qwen-Image-2.1 on RTX 3060 12 GB (int8, 40 steps, native 2K) against gpt-image-2 in the cloud. Edits use real Wikimedia Commons photos.

| Category | Qwen wins | Tie | gpt-image-2 wins |
|---|---:|---:|---:|
| Photography (6) | 0 | 1 | 5 |
| Creativity (6) | 0 | 0 | 6 |
| Photo editing (8) | 1 | 7 | 0 |
| Total | 1 | 8 | 11 |

Photo editing was comparable in these tests, while text-to-image followed long prompts less closely and made more factual / small-text mistakes. Suite, timings and verdicts are under `bench/`. To contribute measurements, use `bash bench/tiers/run.sh` and report `results.json`, your hardware and profile. Testing on a real 10 GB card would be experimental and may fail.

### Repository layout

```text
crates/qil-core   shared setup, engine lifecycle, generation and GPU detection (Rust)
crates/qil-cli    qil command-line tool
app/             Tauri 2 desktop backend and plain HTML/CSS/JS interface
bench/           suites, inputs, results, verdicts and mask-guidance prototype
docs/            benchmark site source
```

### Credits

[Qwen-Image-2.1](https://github.com/QwenLM/Qwen-Image-2.1) by the Qwen team at Alibaba · [ComfyUI](https://github.com/comfyanonymous/ComfyUI) and the [Comfy-Org](https://huggingface.co/Comfy-Org/Qwen-Image-2.1) int8 repacks · [uv](https://github.com/astral-sh/uv) · [Tauri](https://tauri.app). Benchmark photos are from Wikimedia Commons; authors and licenses are in [bench/inputs/credits.json](bench/inputs/credits.json).

### License

The code is [MIT](LICENSE). Qwen-Image-2.1 weights are under the [Qwen Research License](https://huggingface.co/Qwen/Qwen-Image-2.1/blob/main/LICENSE) and are **not licensed for commercial use**. ComfyUI is GPL-3.0 and runs as a separate process. Edits of CC BY-SA benchmark photos are CC BY-SA 4.0.
