(() => {
  const clone = (value) => JSON.parse(JSON.stringify(value));
  const BUILTIN_NAME_KEYS = new Set(["qRemoveBg", "qSnow", "qWatercolor", "qGolden", "qNoPeople", "qCombine"]);

  function displayName(preset, translate) {
    return preset.nameKey && BUILTIN_NAME_KEYS.has(preset.nameKey)
      ? translate(preset.nameKey)
      : preset.name;
  }

  function create({ invoke, getElement, translate, getPrompt, applyPrompt, notify, isBusy }) {
    const $ = getElement;
    let saved = null;
    let draft = null;
    let selectedIndex = -1;
    let loaded = false;
    let loadFailed = false;
    let saving = false;
    let disabled = false;
    let formNameOriginal = "";

    const controls = [
      "promptPresetSelect", "promptPresetManage", "promptPresetName", "promptPresetPrompt",
      "promptPresetNew", "promptPresetDuplicate", "promptPresetDelete", "promptPresetFromCurrent",
      "promptPresetsSave", "promptPresetsCancel", "promptPresetsRetry",
    ];
    const current = () => draft && selectedIndex >= 0 ? draft.presets[selectedIndex] : null;
    const busy = () => disabled || isBusy();

    function updateForm(populate = true) {
      const preset = current();
      if (populate) {
        formNameOriginal = preset ? displayName(preset, translate) : "";
        $("promptPresetName").value = formNameOriginal;
        $("promptPresetPrompt").value = preset?.prompt || "";
      }
      $("promptPresetName").disabled = busy() || !preset;
      $("promptPresetPrompt").disabled = busy() || !preset;
      $("promptPresetDuplicate").disabled = busy() || !preset;
      $("promptPresetDelete").disabled = busy() || !preset;
      $("promptPresetFromCurrent").disabled = busy() || !getPrompt().trim();
      $("promptPresetNew").disabled = busy();
      $("promptPresetsSave").disabled = busy() || saving || loadFailed || !loaded;
      $("promptPresetsRetry").disabled = busy();
    }

    function flushForm() {
      const preset = current();
      if (!preset) return;
      const name = $("promptPresetName").value;
      if (name !== formNameOriginal) {
        preset.name = name;
        preset.nameKey = undefined;
      }
      preset.prompt = $("promptPresetPrompt").value;
      formNameOriginal = displayName(preset, translate);
    }

    function render({ preserveForm = false } = {}) {
      const keepForm = preserveForm && !$("promptPresetsBackdrop").hidden;
      const formName = keepForm ? $("promptPresetName").value : "";
      const formPrompt = keepForm ? $("promptPresetPrompt").value : "";
      const nameWasUnedited = keepForm && formName === formNameOriginal;
      const select = $("promptPresetSelect");
      select.replaceChildren();
      const placeholder = document.createElement("option");
      placeholder.value = "";
      placeholder.textContent = translate("promptPresetChoose");
      select.appendChild(placeholder);
      for (const preset of saved?.presets || []) {
        const option = document.createElement("option");
        option.value = preset.id;
        option.textContent = displayName(preset, translate);
        select.appendChild(option);
      }
      select.value = "";
      select.disabled = busy() || !(saved?.presets?.length > 0);

      const list = $("promptPresetsList");
      list.replaceChildren();
      if (draft && draft.presets.length === 0) {
        const empty = document.createElement("p");
        empty.className = "hint";
        empty.textContent = translate("promptPresetsEmpty");
        list.appendChild(empty);
      }
      for (let index = 0; index < (draft?.presets.length || 0); index += 1) {
        const preset = draft.presets[index];
        const button = document.createElement("button");
        button.type = "button";
        button.className = "prompt-preset-item";
        button.textContent = displayName(preset, translate);
        button.setAttribute("aria-pressed", String(index === selectedIndex));
        button.disabled = busy();
        button.addEventListener("click", () => selectDraft(index));
        list.appendChild(button);
      }
      if (!$("promptPresetsBackdrop").hidden) {
        $("promptPresetsError").hidden = !loadFailed && !$("promptPresetsError").textContent;
        $("promptPresetsRetry").hidden = !loadFailed;
        updateForm();
        if (keepForm && current()) {
          if (!nameWasUnedited) $("promptPresetName").value = formName;
          $("promptPresetPrompt").value = formPrompt;
        }
      }
    }

    function showError(message) {
      const region = $("promptPresetsError");
      region.textContent = message;
      region.hidden = false;
    }

    async function load() {
      const retryingFailedLoad = loadFailed;
      try {
        const document = await invoke("prompt_presets_load");
        saved = clone(document);
        loaded = true;
        loadFailed = false;
        if (retryingFailedLoad && draft) {
          draft = clone(saved);
          selectedIndex = draft.presets.length ? 0 : -1;
        }
        $("promptPresetsError").textContent = "";
        $("promptPresetsError").hidden = true;
        render();
      } catch (error) {
        loaded = false;
        loadFailed = true;
        if (!$("promptPresetsBackdrop").hidden) showError(translate("promptPresetsLoadFailed"));
        else notify(translate("promptPresetsLoadFailed"));
        render();
      }
    }

    function open() {
      if (busy()) return;
      draft = clone(saved || { version: 1, presets: [] });
      selectedIndex = draft.presets.length ? 0 : -1;
      $("promptPresetsError").textContent = "";
      $("promptPresetsError").hidden = true;
      $("promptPresetsBackdrop").hidden = false;
      render();
      updateForm();
      $("promptPresetName").focus();
    }

    function close() {
      if ($("promptPresetsBackdrop").hidden) return;
      $("promptPresetsBackdrop").hidden = true;
      draft = null;
      selectedIndex = -1;
      $("promptPresetManage").focus();
    }

    function selectDraft(index) {
      if (busy() || !draft || index < 0 || index >= draft.presets.length) return;
      flushForm();
      selectedIndex = index;
      render();
      updateForm();
    }

    function freshId() {
      let id;
      do { id = `user-${crypto.randomUUID()}`; }
      while (draft.presets.some((preset) => preset.id === id));
      return id;
    }

    function addPreset(preset) {
      if (busy() || !draft) return;
      flushForm();
      draft.presets.push(preset);
      selectedIndex = draft.presets.length - 1;
      render();
      updateForm();
      $("promptPresetName").focus();
    }

    async function manage() {
      if (busy()) return;
      if (!loaded || loadFailed) {
        await load();
        if (!loaded) {
          draft = { version: 1, presets: [] };
          selectedIndex = -1;
          $("promptPresetsBackdrop").hidden = false;
          showError(translate("promptPresetsLoadFailed"));
          render();
          return;
        }
      }
      open();
    }

    $("promptPresetSelect").addEventListener("change", () => {
      if (busy()) { $("promptPresetSelect").value = ""; return; }
      const preset = saved?.presets.find((item) => item.id === $("promptPresetSelect").value);
      if (!preset) return;
      applyPrompt(preset.prompt);
      $("promptPhotos").focus();
      $("promptPresetSelect").value = "";
    });
    $("promptPresetManage").addEventListener("click", manage);
    $("promptPresetsCancel").addEventListener("click", close);
    $("promptPresetsBackdrop").addEventListener("click", (event) => {
      if (event.target === $("promptPresetsBackdrop")) close();
    });
    $("promptPresetNew").addEventListener("click", () => addPreset({ id: freshId(), name: "", prompt: "" }));
    $("promptPresetFromCurrent").addEventListener("click", () => {
      const prompt = getPrompt();
      if (!prompt.trim()) return;
      addPreset({ id: freshId(), name: translate("promptPresetFromCurrent"), prompt });
    });
    $("promptPresetDuplicate").addEventListener("click", () => {
      flushForm();
      const preset = current();
      if (!preset || busy()) return;
      addPreset({
        id: freshId(),
        name: `${displayName(preset, translate)} — ${translate("promptPresetCopyName")}`,
        prompt: preset.prompt,
      });
    });
    $("promptPresetDelete").addEventListener("click", () => {
      if (busy() || !current()) return;
      draft.presets.splice(selectedIndex, 1);
      selectedIndex = Math.min(selectedIndex, draft.presets.length - 1);
      render();
      updateForm();
    });
    $("promptPresetName").addEventListener("input", () => {
      $("promptPresetsError").hidden = true;
    });
    $("promptPresetPrompt").addEventListener("input", () => { $("promptPresetsError").hidden = true; });
    $("promptPresetsRetry").addEventListener("click", load);
    $("promptPresetsSave").addEventListener("click", async () => {
      if (busy() || saving || loadFailed || !draft) return;
      flushForm();
      const invalidName = draft.presets.some((preset) => !preset.name.trim());
      const invalidPrompt = draft.presets.some((preset) => !preset.prompt.trim());
      if (invalidName || invalidPrompt) {
        showError(translate(invalidName ? "promptPresetNameRequired" : "promptPresetPromptRequired"));
        return;
      }
      saving = true;
      updateForm();
      try {
        saved = clone(await invoke("prompt_presets_save", { document: clone(draft) }));
        loaded = true;
        render();
        close();
      } catch (error) {
        showError(`${translate("promptPresetsSaveFailed")}${String(error)}`);
      } finally {
        saving = false;
        updateForm();
        render();
      }
    });

    return Object.freeze({
      load,
      render,
      setDisabled(value) {
        disabled = !!value;
        for (const id of controls) $(id).disabled = disabled;
        $("promptPresetSelect").disabled = disabled || !(saved?.presets?.length > 0);
        $("promptPresetsList").querySelectorAll("button").forEach((button) => { button.disabled = disabled; });
        updateForm(false);
      },
      isOpen: () => !$("promptPresetsBackdrop").hidden,
      handleKeydown(event) {
        if (!this.isOpen()) return;
        if (event.key === "Escape") {
          event.preventDefault();
          close();
          return;
        }
        if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
          event.preventDefault();
          return;
        }
        if (event.key === "Tab") {
          const focusable = $("promptPresetsDialog").querySelectorAll(
            "button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled)",
          );
          const enabled = [...focusable].filter((element) => !element.disabled && !element.hidden);
          const first = enabled[0];
          const last = enabled.at(-1);
          if (event.shiftKey && document.activeElement === first) {
            event.preventDefault();
            last?.focus();
          } else if (!event.shiftKey && document.activeElement === last) {
            event.preventDefault();
            first?.focus();
          }
        }
      },
    });
  }

  const api = Object.freeze({ create, displayName });
  if (typeof module === "object" && module.exports) module.exports = api;
  else window.QilPromptPresets = api;
})();
