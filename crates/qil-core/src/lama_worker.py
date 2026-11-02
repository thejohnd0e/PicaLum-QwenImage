"""LaMa erase worker. Runs in the app's Python environment.

Usage: python lama_worker.py MODEL SOURCE MASK OUTPUT
The source keeps its upright EXIF orientation; the mask must have the same size.
"""

import json
import sys

import numpy as np
import torch
from PIL import Image, ImageOps

MODULO = 8


def fail(message):
    print(message, file=sys.stderr)
    raise SystemExit(1)


def main():
    if len(sys.argv) != 5:
        fail("usage: lama_worker.py MODEL SOURCE MASK OUTPUT")
    model_path, source_path, mask_path, output_path = sys.argv[1:5]

    source = ImageOps.exif_transpose(Image.open(source_path)).convert("RGB")
    mask = Image.open(mask_path).convert("L")
    if mask.size != source.size:
        fail(f"mask size {mask.size} does not match source {source.size}")

    width, height = source.size
    padded_width = ((width + MODULO - 1) // MODULO) * MODULO
    padded_height = ((height + MODULO - 1) // MODULO) * MODULO

    image = np.asarray(source, dtype=np.float32) / 255.0
    painted = (np.asarray(mask, dtype=np.uint8) > 0).astype(np.float32)
    if not painted.any():
        fail("empty mask")
    image = np.pad(image, ((0, padded_height - height), (0, padded_width - width), (0, 0)))
    painted = np.pad(painted, ((0, padded_height - height), (0, padded_width - width)))

    tensor_image = torch.from_numpy(image).permute(2, 0, 1).unsqueeze(0)
    tensor_mask = torch.from_numpy(painted).unsqueeze(0).unsqueeze(0)
    model = torch.jit.load(model_path, map_location="cpu").eval()

    device = "cuda" if torch.cuda.is_available() else "cpu"
    try:
        model = model.to(device)
        tensor_image = tensor_image.to(device)
        tensor_mask = tensor_mask.to(device)
        with torch.no_grad():
            result = model(tensor_image, tensor_mask)
    except torch.cuda.OutOfMemoryError:
        torch.cuda.empty_cache()
        model = model.to("cpu")
        tensor_image = tensor_image.to("cpu")
        tensor_mask = tensor_mask.to("cpu")
        with torch.no_grad():
            result = model(tensor_image, tensor_mask)

    result = result[0, :, :height, :width].clamp(0, 1).mul(255).round()
    result = result.to(torch.uint8).permute(1, 2, 0).cpu().numpy()
    Image.fromarray(result, "RGB").save(output_path)
    print(json.dumps({"width": width, "height": height}))


if __name__ == "__main__":
    main()
