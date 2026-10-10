// One mask-guided edit run: snapshot the request, call the backend once, and put
// only the final composite into the session history. DOM-free so it can be tested
// with a mocked `invoke`; app.js supplies the view hooks.
(() => {
  // deps: { invoke, state, translate,
  //         lockedSeed() -> number|null,
  //         hooks: { begin(), end(), applyResult(item), notify(text, ok) } }
  function create(deps) {
    const { invoke, state, translate: t, hooks } = deps;

    async function run({ sourcePath, maskPngBase64, instruction, featherPx, references = [] }) {
      if (state.busy) return { error: t("maskBusy") };
      const seed = deps.lockedSeed();
      const preset = state.preset;
      const runId = ++state.generationRun;
      state.stopRequested = false;
      state.busy = true;
      hooks.begin();
      try {
        const r = await invoke("generate_masked_edit", {
          args: { sourcePath, maskPngBase64, instruction, preset, seed, featherPx, references },
        });
        const item = {
          path: r.path,
          meta: {
            mode: "masked-edit",
            prompt: instruction,
            preset,
            seed: r.seed,
            width: r.width,
            height: r.height,
            seconds: r.seconds,
            references: [sourcePath, ...references],
          },
        };
        state.sessionOutputs.add(item.path);
        state.history.unshift(item);
        hooks.applyResult(item);
        return { ok: true, item };
      } catch (e) {
        const msg = String(e);
        if (msg === "Cancelled" || runId !== state.generationRun) {
          hooks.notify(t("cancelled"), true);
          return { error: t("cancelled") };
        }
        return { error: msg };
      } finally {
        state.busy = false;
        state.stopRequested = false;
        hooks.end();
      }
    }

    return Object.freeze({ run });
  }

  const api = Object.freeze({ create });
  if (typeof module === "object" && module.exports) module.exports = api;
  else window.QilMaskedEditRun = api;
})();
