// One LaMa erase run: snapshot the source and mask, call the backend once,
// and publish only the final PNG into the session history.
(() => {
  // deps: { invoke, state, translate, hooks: { begin, end, applyResult, notify } }
  function create(deps) {
    const { invoke, state, translate: t, hooks } = deps;

    async function run({ sourcePath, maskPngBase64 }) {
      if (state.busy) return { error: t("maskBusy") };
      if (!sourcePath || !maskPngBase64) return { error: t("maskEmpty") };
      const runId = ++state.generationRun;
      state.stopRequested = false;
      state.busy = true;
      hooks.begin();
      try {
        const result = await invoke("erase_image", {
          args: { sourcePath, maskPngBase64 },
        });
        if (runId !== state.generationRun) {
          hooks.notify(t("cancelled"), true);
          return { error: t("cancelled") };
        }
        const item = {
          path: result.path,
          meta: {
            mode: "erase",
            prompt: t("erase"),
            model: "LaMa",
            width: result.width,
            height: result.height,
            seconds: result.seconds,
            references: [sourcePath],
          },
        };
        state.sessionOutputs.add(item.path);
        state.history.unshift(item);
        hooks.applyResult(item);
        return { ok: true, item };
      } catch (error) {
        const message = String(error);
        if (message === "Cancelled" || runId !== state.generationRun) {
          hooks.notify(t("cancelled"), true);
          return { error: t("cancelled") };
        }
        return { error: message };
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
  else window.QilEraseRun = api;
})();
