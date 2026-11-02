(() => {
  const PRESETS = new Set(["draft", "standard", "max"]);
  const PRESET_IDS = { draft: "qualityPresetDraft", standard: "qualityPresetStandard", max: "qualityPresetMax" };
  const PRESET_NAMES = { draft: "pDraft", standard: "pStandard", max: "pMax" };
  const STORAGE_KEY = "quality.settings";
  const OPEN_KEY = "quality.open";

  function create({ getElement, invoke, translate = (key) => key, storage = {}, onChange = () => {} }) {
    const $ = getElement;
    let context = { mode: "text", aspect: "1:1", sourcePath: null };
    let basePreset = "standard";
    let values = { steps: 30, width: 1280, height: 1280 };
    let valid = true;
    let pending = false;
    let custom = false;
    let fixedSize = null;
    let serial = 0;
    let initialized = false;
    let busy = false;

    const snapshot = () => ({
      preset: basePreset, steps: values.steps,
      width: values.width, height: values.height,
      custom, valid: valid && !pending, pixelBudget: values.width && values.height ? values.width * values.height : undefined,
    });
    const save = () => storage.set?.(STORAGE_KEY, JSON.stringify({ preset: basePreset, ...values, custom }));
    const changed = () => { save(); render(); onChange(snapshot()); };

    function render() {
      const photo = context.mode === "photos";
      $("qualitySteps").value = valid || !$("qualitySteps").value ? String(values.steps) : $("qualitySteps").value;
      $("qualityWidth").value = String(values.width ?? "");
      $("qualityHeight").value = String(values.height ?? "");
      $("qualityWidth").disabled = busy || photo;
      $("qualityHeight").disabled = busy || photo;
      $("qualitySteps").disabled = busy;
      $("qualitySizePair").hidden = false;
      for (const [preset, id] of Object.entries(PRESET_IDS)) {
        $(id).setAttribute("aria-checked", String(!custom && basePreset === preset));
      }
      $("qualityStatus").textContent = photo
        ? (context.sourcePath ? translate("qualitySourceShort") : "")
        : `${megabytes()} MP`;
      const lowResolution = !photo && (values.width < 1024 || values.height < 1024);
      $("qualityWarning").hidden = !photo && !lowResolution;
      $("qualityWarning").textContent = photo
        ? (context.sourcePath ? translate("qualityPhotoHint") : translate("qualityNoSource"))
        : lowResolution ? translate("qualityLowResolution") : "";
      $("qualityError").textContent = valid ? "" : translate("qualityInvalid");
      $("qualityError").hidden = valid || pending;
      const size = photo && !context.sourcePath ? translate("qualityNoSourceShort") : `${values.width}×${values.height}`;
      const presetName = custom ? translate("qualityCustom") : translate(PRESET_NAMES[basePreset]);
      $("qualitySummary").textContent = `${presetName} · ${values.steps} · ${size}`;
      $("qualityPresetMeta").textContent = custom ? translate("qualityCustomHint") : presetMeta();
    }

    function presetMeta() {
      const button = $(PRESET_IDS[basePreset]);
      return (context.mode === "photos" ? button.dataset.e : button.dataset.t) || "";
    }

    function megabytes() {
      return ((values.width * values.height) / 1_000_000).toFixed(1);
    }

    function request(preset = basePreset, overrides = {}) {
      return {
        mode: context.mode === "photos" && !context.sourcePath ? "text" : context.mode,
        prompt: "", images: context.mode === "photos" && context.sourcePath ? [context.sourcePath] : [],
        preset, aspect: context.aspect,
        steps: overrides.steps, width: overrides.width, height: overrides.height,
      };
    }

    async function resolve(preset, overrides = {}) {
      const token = ++serial;
      pending = true;
      valid = true;
      let result;
      try { result = await invoke("generation_settings_resolve", { request: request(preset, overrides) }); }
      catch (error) {
        if (token === serial) { pending = false; valid = false; render(); onChange(snapshot()); }
        throw error;
      }
      if (token !== serial) return false;
      values = { steps: result.steps, width: result.width, height: result.height };
      pending = false;
      valid = true;
      return true;
    }

    async function selectPreset(preset) {
      if (!PRESETS.has(preset)) return;
      basePreset = preset;
      custom = false;
      const ok = await resolve(preset);
      if (ok) changed();
    }

    async function setContext(next) {
      const previous = context;
      context = { ...context, ...next };
      const photo = context.mode === "photos";
      const wasFixed = previous.mode !== "photos";
      if (wasFixed && photo) fixedSize = { width: values.width, height: values.height };

      const overrides = { steps: values.steps };
      if (!photo) {
        const basis = !wasFixed && fixedSize ? fixedSize : { width: values.width, height: values.height };
        if (previous.aspect !== context.aspect) {
          const ratio = context.aspect.split(":").map(Number);
          const area = basis.width * basis.height;
          const width = Math.max(256, Math.round(Math.sqrt(area * ratio[0] / ratio[1]) / 32) * 32);
          const height = Math.max(256, Math.round((area / width) / 32) * 32);
          overrides.width = width;
          overrides.height = height;
        } else {
          overrides.width = basis.width;
          overrides.height = basis.height;
        }
      }

      if (photo && !context.sourcePath) {
        serial++;
        pending = false;
        changed();
      } else {
        const ok = await resolve(basePreset, overrides);
        if (ok) changed();
      }
      render();
    }

    function edit(field) {
      serial++;
      pending = false;
      const input = $(`quality${field[0].toUpperCase()}${field.slice(1)}`);
      const raw = input.value;
      const number = Number(raw);
      const parsed = Number.isInteger(number) && number >= (field === "steps" ? 1 : 256)
        && number <= (field === "steps" ? 100 : 4096)
        && (field === "steps" || number % 32 === 0);
      if (!parsed) {
        valid = false;
        input.setCustomValidity(translate("qualityInvalid"));
        $("qualityError").textContent = translate("qualityInvalid");
        $("qualityError").hidden = false;
        onChange(snapshot());
        return;
      }
      input.setCustomValidity("");
      const next = { ...values, [field]: number };
      if (field === "width" || field === "height") {
        const [ratioW, ratioH] = context.aspect.split(":").map(Number);
        const paired = field === "width"
          ? Math.max(256, Math.round((number * ratioH / ratioW) / 32) * 32)
          : Math.max(256, Math.round((number * ratioW / ratioH) / 32) * 32);
        next[field === "width" ? "height" : "width"] = paired;
      }
      if (next.width * next.height > 4_500_000) {
        valid = false;
        input.setCustomValidity(translate("qualityAreaInvalid"));
        $("qualityError").textContent = translate("qualityAreaInvalid");
        $("qualityError").hidden = false;
        onChange(snapshot());
        return;
      }
      values = next;
      valid = true;
      custom = true;
      changed();
    }

    async function init(initialContext) {
      context = { ...context, ...initialContext };
      const details = $("qualityPresets");
      if (details) details.open = storage.get?.(OPEN_KEY) === "1";
      let saved = null;
      try { saved = JSON.parse(storage.get?.(STORAGE_KEY) || "null"); } catch { saved = null; }
      const legacyPreset = storage.get?.("preset");
      const preset = PRESETS.has(saved?.preset) ? saved.preset : PRESETS.has(legacyPreset) ? legacyPreset : "standard";
      basePreset = preset;
      if (saved && Number.isInteger(saved.steps) && Number.isInteger(saved.width) && Number.isInteger(saved.height)) {
        values = { steps: saved.steps, width: saved.width, height: saved.height };
        custom = saved.custom === true;
        let result = false;
        try { result = await resolve(preset, context.mode === "photos" ? { steps: values.steps } : values); } catch { /* Invalid saved settings fall back to their base preset. */ }
        if (!result) {
          custom = false;
          await resolve(preset);
        } else if (saved.custom) {
          // Preserve the saved values exactly; the resolver has validated them.
          values = { steps: saved.steps, width: saved.width, height: saved.height };
        }
      } else {
        await resolve(preset);
      }
      initialized = true;
      render();
      changed();
      return snapshot();
    }

    for (const preset of PRESETS) $(PRESET_IDS[preset]).addEventListener("click", () => selectPreset(preset));
    for (const field of ["steps", "width", "height"]) $(
      `quality${field[0].toUpperCase()}${field.slice(1)}`,
    ).addEventListener("input", () => edit(field));
    $("qualityPresets").addEventListener("toggle", () => {
      storage.set?.(OPEN_KEY, $("qualityPresets").open ? "1" : "0");
    });

    function setBusy(value) {
      const next = Boolean(value);
      if (next === busy) return;
      busy = next;
      render();
    }
    return Object.freeze({ init, selectPreset, setContext, snapshot, render, setBusy, isInitialized: () => initialized });
  }

  const api = Object.freeze({ create });
  if (typeof module === "object" && module.exports) module.exports = api;
  else window.QilQualityControls = api;
})();
