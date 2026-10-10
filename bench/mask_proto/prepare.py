"""Prepare inputs for the mask-guidance prototype (plan: mask-guided editing, Task 1).

Writes, under bench/mask_proto/work/ (git-ignored):
  inputs/<case>_orig.png    source, EXIF-normalized
  inputs/<case>_mask.png    binary mask shown as a black/white RGB image (white = edit)
  inputs/<case>_marked.png  source with a red outline + light tint on the region
  mask/<case>.png           grayscale mask, source-sized (used by composite.py)
  suite.json                one test per case x candidate

Candidates:
  A_mask2   image_1 = original, image_2 = mask picture, prompt ties <image2> to the region
  B_marked  image_1 = marked original, prompt explains the red mark
  C_control image_1 = original, instruction only (no spatial guidance)

Usage (repo root):  python bench/mask_proto/prepare.py
"""
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageOps

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
WORK = HERE / "work"
SRC_DIR = ROOT / "bench" / "inputs"


def region_mask(size, shape, box):
    w, h = size
    x0, y0, x1, y1 = (box[0] * w, box[1] * h, box[2] * w, box[3] * h)
    m = Image.new("L", size, 0)
    d = ImageDraw.Draw(m)
    (d.ellipse if shape == "ellipse" else d.rectangle)([x0, y0, x1, y1], fill=255)
    return m


def marked(src, mask):
    tint = Image.new("RGB", src.size, (255, 0, 0))
    out = Image.composite(Image.blend(src, tint, 0.25), src, mask)
    # outline = mask minus its erosion
    from PIL import ImageFilter

    width = max(4, src.size[0] // 300)
    inner = mask.filter(ImageFilter.MinFilter(width * 2 + 1))
    edge = Image.fromarray(
        __import__("numpy").clip(
            __import__("numpy").asarray(mask, dtype="int16")
            - __import__("numpy").asarray(inner, dtype="int16"),
            0,
            255,
        ).astype("uint8")
    )
    return Image.composite(Image.new("RGB", src.size, (255, 0, 0)), out, edge)


def main():
    cases = json.loads((HERE / "cases.json").read_text(encoding="utf-8"))
    for sub in ("inputs", "mask"):
        (WORK / sub).mkdir(parents=True, exist_ok=True)
    suite = []
    for c in cases:
        src = ImageOps.exif_transpose(Image.open(SRC_DIR / c["source"])).convert("RGB")
        mask = region_mask(src.size, c["shape"], c["box"])
        cid = c["id"]
        src.save(WORK / "inputs" / f"{cid}_orig.png")
        mask.save(WORK / "mask" / f"{cid}.png")
        mask.convert("RGB").save(WORK / "inputs" / f"{cid}_mask.png")
        marked(src, mask).save(WORK / "inputs" / f"{cid}_marked.png")
        ins = c["instruction"]
        suite += [
            {
                "id": f"{cid}__A_mask2",
                "mode": "edit",
                "images": [f"{cid}_orig.png", f"{cid}_mask.png"],
                "prompt": (
                    "<image1> is the photo to edit. <image2> is a black and white mask of the same photo: "
                    "only the white area of <image2> may change. "
                    f"{ins} Apply this change only inside the white area of <image2>. "
                    "Keep everything else in <image1> exactly the same. Do not show the mask in the result."
                ),
            },
            {
                "id": f"{cid}__B_marked",
                "mode": "edit",
                "images": [f"{cid}_marked.png"],
                "prompt": (
                    "The photo contains a red outline and red tint marking the region to edit. "
                    f"{ins} Apply the change only inside the marked region. "
                    "Remove the red outline and tint, and keep everything outside the region exactly the same."
                ),
            },
            {
                "id": f"{cid}__C_control",
                "mode": "edit",
                "images": [f"{cid}_orig.png"],
                "prompt": f"{ins} Keep everything else exactly the same.",
            },
        ]
    (WORK / "suite.json").write_text(json.dumps(suite, indent=2), encoding="utf-8")
    print(f"{len(cases)} cases, {len(suite)} tests -> {WORK}")


if __name__ == "__main__":
    main()
