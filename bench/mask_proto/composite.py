"""Prototype composite + exact-pixel check + contact sheets for the mask prototype.

For each raw result in work/out/seed_*/<case>__<cand>.png:
  - resize to the source size if the engine changed it (LANCZOS; recorded in the report),
  - composite onto the original through the case mask (hard and 8px inward feather),
  - verify pixels outside the mask equal the source exactly,
  - build a contact sheet: original | raw | composite(hard) | composite(feather) per candidate.

Writes work/sheets/<case>__seed_<n>.jpg and work/report.json.
Usage (repo root): python bench/mask_proto/composite.py
"""
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

HERE = Path(__file__).resolve().parent
WORK = HERE / "work"
FEATHER = 8


def inward_feather(mask, px):
    """Inward-only soft mask: weight ramps with distance from the nearest 0 pixel."""
    if px == 0:
        return np.asarray(mask, dtype=np.float32) / 255.0
    m = np.asarray(mask, dtype=np.uint8)
    hard = m > 0
    # Approximate inward distance with repeated erosion (cheap, no scipy needed).
    img = Image.fromarray((hard * 255).astype("uint8"))
    w = np.zeros(hard.shape, dtype=np.float32)
    for i in range(1, px + 1):
        img = img.filter(ImageFilter.MinFilter(3))
        w += np.asarray(img, dtype=np.float32) / 255.0
    return np.where(hard, np.minimum((w + 1) / px, 1.0), 0.0)


def composite(src, gen, weight):
    s = np.asarray(src, dtype=np.float32)
    g = np.asarray(gen, dtype=np.float32)
    w = weight[..., None]
    out = np.rint(s * (1 - w) + g * w).astype("uint8")
    out[weight == 0] = np.asarray(src)[weight == 0]  # exact copy outside mask
    out[weight == 1] = np.asarray(gen)[weight == 1]
    return Image.fromarray(out)


def label(img, text):
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, 8 * len(text) + 8, 18], fill=(0, 0, 0))
    d.text((4, 3), text, fill=(255, 255, 255))
    return img


def main():
    (WORK / "sheets").mkdir(exist_ok=True)
    report = []
    for seed_dir in sorted((WORK / "out").glob("seed_*")):
        seed = seed_dir.name
        per_case = {}
        for raw_path in sorted(seed_dir.glob("*__*.png")):
            case, cand = raw_path.stem.split("__", 1)
            src = Image.open(WORK / "inputs" / f"{case}_orig.png").convert("RGB")
            mask = Image.open(WORK / "mask" / f"{case}.png").convert("L")
            raw = Image.open(raw_path).convert("RGB")
            resized = raw.size != src.size
            gen = raw.resize(src.size, Image.LANCZOS) if resized else raw
            hard = composite(src, gen, inward_feather(mask, 0))
            soft = composite(src, gen, inward_feather(mask, FEATHER))
            outside = np.asarray(mask) == 0
            exact = all(
                np.array_equal(np.asarray(c)[outside], np.asarray(src)[outside])
                for c in (hard, soft)
            )
            report.append(
                {
                    "seed": seed,
                    "case": case,
                    "candidate": cand,
                    "raw_size": raw.size,
                    "source_size": src.size,
                    "resized": resized,
                    "outside_mask_exact": bool(exact),
                    "raw": str(raw_path.relative_to(WORK)),
                }
            )
            per_case.setdefault(case, []).append((cand, src, gen, hard, soft))
        for case, rows in per_case.items():
            tw = 360
            th = int(rows[0][1].size[1] * tw / rows[0][1].size[0])
            sheet = Image.new("RGB", (tw * 4, th * len(rows)), (30, 30, 30))
            for r, (cand, src, gen, hard, soft) in enumerate(rows):
                for c, (name, im) in enumerate(
                    [("original", src), (f"{cand} raw", gen), ("composite hard", hard), (f"composite feather {FEATHER}", soft)]
                ):
                    t = label(im.resize((tw, th)), name)
                    sheet.paste(t, (c * tw, r * th))
            sheet.save(WORK / "sheets" / f"{case}__{seed}.jpg", quality=88)
    (WORK / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    bad = [r for r in report if not r["outside_mask_exact"]]
    print(f"{len(report)} results, outside-mask exact: {len(report) - len(bad)}/{len(report)}")
    if bad:
        raise SystemExit("outside-mask pixels changed for some results")


if __name__ == "__main__":
    main()
