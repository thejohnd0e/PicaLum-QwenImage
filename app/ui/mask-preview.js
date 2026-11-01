(() => {
  function create({ invoke, onResult = () => {}, onBusy = () => {}, onError = () => {} }) {
    let active = false;
    let pending = null;
    let latest = null;
    let source = null;
    let generation = 0;

    function settle(item, value) {
      item?.waiters.forEach((resolve) => resolve(value));
    }

    async function drain() {
      if (active) return;
      active = true;
      onBusy(true);
      try {
        while (pending) {
          const item = pending;
          pending = null;
          try {
            const result = await invoke({
              maskPngBase64: item.maskPngBase64,
              featherPx: item.featherPx,
              featherMode: item.featherMode,
            });
            const current = item.generation === generation && item.revision === latest?.revision && item.source === source;
            if (current) onResult(result, item);
            settle(item, current ? result : null);
          } catch (error) {
            const current = item.generation === generation && item.revision === latest?.revision && item.source === source;
            if (current) onError(error, item);
            settle(item, null);
          }
        }
      } finally {
        active = false;
        onBusy(false);
        // Cover a request queued between the loop condition and finally.
        if (pending) void drain();
      }
    }

    function request({ maskPngBase64, featherPx = 8, featherMode = "inward", revision, source: nextSource = source }) {
      const requestGeneration = generation;
      source = nextSource;
      const item = { maskPngBase64, featherPx, featherMode, revision, source, generation: requestGeneration, waiters: [] };
      latest = item;
      const promise = new Promise((resolve) => item.waiters.push(resolve));
      if (pending) settle(pending, null);
      pending = item;
      void drain();
      return promise;
    }

    function invalidate(nextSource = null) {
      generation++;
      source = nextSource;
      latest = null;
      if (pending) settle(pending, null);
      pending = null;
    }

    return Object.freeze({ request, invalidate, get busy() { return active; } });
  }

  const api = Object.freeze({ create });
  if (typeof module === "object" && module.exports) module.exports = api;
  else window.QilMaskPreview = api;
})();
