# Negative-prompt effectiveness investigation

**Status:** Deferred follow-up. The desktop field was removed because it did not produce the expected result in the user's test. This is not evidence that Qwen-Image-2.1 categorically lacks negative-prompt support.

## Questions to answer

- [ ] Record the user's exact positive/negative prompts, mode, model/checkpoint and quantization, seed, steps, CFG, and other settings where available.
- [ ] Verify a request originating in desktop UI separately from CLI; trace prompt text through Tauri and `qil-core` into the actual ComfyUI graph. The masked-edit path currently sets `negative` to an empty string; keep that known limitation separate from regular text/photo generation.
- [ ] Re-check official Qwen-Image-2.1, Diffusers and ComfyUI documentation/source, distinguishing original and Turbo checkpoints, hosted API and local model, and CFG defaults from the existence of true CFG.
- [ ] Existing local A/B: CFG 3.0, seed 424242, Draft preset; the positive prompt explicitly requested a red cube and blue sphere. Negative prompts targeted each object. The graph changed only its negative text; outputs changed pixels but did not remove the requested objects. This conflicting-prompt, one-seed result does not establish that negative prompts are unsupported.
- [ ] With separate GPU authorization, compare empty and targeted negatives with non-conflicting positive prompts, fixed seeds/settings and several examples/repeats. Distinguish pixel changes from actual semantic suppression.
- [ ] If feasible, compare ComfyUI against reference Diffusers using compatible weights, precision and sampling settings; state unavoidable differences. Do not download or load alternate large checkpoints without authorization.
- [ ] Publish evidence and a bounded conclusion: UI wiring defect, backend difference, quantization effect, model behavior, or inconclusive. Recommend restoring a UI control only if there is a demonstrated useful effect.

## Prior test artifacts and sources

- Artifacts (temporary and may no longer exist): `C:\Users\la\AppData\Local\Temp\opencode\qwen-negative-20261102-020445`.
- https://github.com/QwenLM/Qwen-Image-2.1
- https://huggingface.co/docs/diffusers/api/pipelines/qwenimage21
- https://docs.comfy.org/tutorials/image/qwen/qwen-image-2-1
