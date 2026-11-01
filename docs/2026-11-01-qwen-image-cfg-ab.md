# A/B: classifier-free guidance on Qwen-Image-2.1 (2026-11-01)

Question: `build_graph` hardcoded `"cfg": 1.0` and sent `negative_prompt: ""`, so guidance never
took part in generation. Does raising it make the model follow the prompt better on a 12 GB card?

## Change under test

- `GenRequest` gained `cfg: f64` (default `1.0`) and `negative: String` (default empty), both
  `#[serde(default)]`, so app requests and existing callers keep the historic behaviour byte for byte.
- `build_graph` takes both and puts them into `KSampler.cfg` and `TextEncodeQwenImage21.negative_prompt`.
- CLI: `qil generate|edit|surprise --cfg <f64> --negative <text>`. `bench` stays at `1.0`/empty on purpose.
- Masked edit and the GPU verification request keep `1.0`/empty.

## Method

- Hardware: RTX 3060 12 GB, driver 580+, Windows; profile `balanced` (`qwen3vl_8b_int8_convrot`),
  DiT `qwen_image_2.1_int8_convrot`, VAE bf16.
- Preset `standard`: 1280×1280, 30 steps, `euler` / `simple`, 1.6 MP. Seed **42** for every run.
- Prompt (identical in all runs): a watchmaker workshop with a plaque reading "PRECISION", asking for a
  hyperrealistic 50 mm photograph. Negative prompt for runs B and C: `blurry, low detail, deformed hands,
  extra fingers, garbled text, watermark, oversaturated, plastic skin`.
- One image per setting (n = 1). This is a smoke comparison, not a benchmark.

## Results

| Run | cfg | Negative | Time | Notes |
| --- | ---: | --- | ---: | --- |
| A | 1.0 | — | 185 s | Correct scene and plaque text; busier background, the loupe and hands read less naturally |
| B | 3.0 | yes | 196 s | Cleaner composition, plaque text crisp, hands more plausible |
| C | 4.5 | yes | 191 s | Most coherent of the three: best plaque placement, best hands, calmer background |

Images: `F:\ab-test\a-cfg1.0.png`, `b-cfg3.0.png`, `c-cfg4.5.png`.

## Observations

- Guidance cost about **+3–6 % wall time**, not the ~2× a textbook CFG batch would suggest; the run stays
  within the same VRAM budget (no OOM, engine not restarted between runs).
- Text rendering was already correct at cfg 1.0, so the gain is composition, hands and background order,
  not text.
- cfg 4.5 was not "burned": colours and contrast stayed normal, unlike distilled turbo models that over-cook
  above ~2.5. Qwen-Image-2.1 tolerates 3–4.5 here.

## Verdict

Keep `1.0` as the default (no behaviour change for the app), and use **3.0–4.5** when a prompt has many
constraints (objects, positions, readable text). Worth exposing in the UI as a "Prompt adherence" slider.

Limits: one prompt, one seed, one card, one quantisation. Face/hand detail at 1.6 MP is still the weak spot;
`-p max` (2K, 40 steps) was not measured.

## After the controls landed (CLI smoke, 2026-11-01)

The UI controls are covered by request-snapshot tests, but this repeat was run through the newly built CLI
rather than by clicking through the desktop window. Both requests used the exact prompt above, seed 42,
standard preset, and the same RTX 3060/runtime. Outputs are `F:\ab-test\after-ui-cfg1.0-exact.png` and
`F:\ab-test\after-ui-cfg3.0-exact.png`.

| CFG | Negative | Time | Visual note |
| ---: | --- | ---: | --- |
| 1.0 | — | 88 s | Correct scene and legible “PRECISION” plaque, but the surrounding plaque text is garbled and the bench is less orderly. |
| 3.0 | Same as run B | 167 s | “PRECISION” is crisp; the composition is more deliberate and orderly. |

This single CLI smoke run supports the wiring and shows a qualitative change, not a benchmark or proof of
the UI interaction. Its measured wall-time increase was about 90%, unlike the +3–6% in the earlier A/B;
do not use these two timings to estimate CFG cost. The desktop slider and negative-prompt persistence still
need a manual UI check. The generated images also differ from the earlier run because the model is not
deterministic across engine/session conditions despite the same seed.
