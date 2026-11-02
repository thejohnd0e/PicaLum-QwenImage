# Third-party notices

## LaMa — resolution-robust large-mask inpainting

- Project: https://github.com/advimman/lama
- Copyright: Samsung Research
- License: Apache License 2.0 — https://github.com/advimman/lama/blob/main/LICENSE
- Weights used by PicaLum: `big-lama.pt`
  - Publisher: Sanster, https://github.com/Sanster/models/releases/tag/add_big_lama
  - Size: 205,669,692 bytes
  - SHA-256: `344c77bbcb158f17dd143070d1e789f38a66c04202311ae3a258ef66667a9ea9`
  - The same artifact hash and size are published by the `fofr/comfyui` and `uwg/modelz_base` Hugging Face mirrors; the upstream LaMa repository links the `smartywu/big-lama` model card, which is licensed Apache-2.0.
- The weights are downloaded only after the user presses **Download model** in the mask editor. The model file is not bundled in the installer.

## IOPaint

IOPaint is not bundled and is not a runtime dependency. Its Apache-2.0 implementation was inspected to confirm the LaMa input format and model provenance; no IOPaint source code is copied into this repository.
