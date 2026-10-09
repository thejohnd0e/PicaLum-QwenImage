// PicaLum QwenImage — UI logic. Plain JS with withGlobalTauri (no bundler).
(() => {
  const T = window.__TAURI__;
  const invoke = T.core.invoke;
  const listen = T.event.listen;
  const fileSrc = (p) => T.core.convertFileSrc(p);
  const $ = (id) => document.getElementById(id);
  const t = (...a) => window.i18n.t(...a);

  const MAX_REFS = 10;
  const QUICK = [
    ["qRemoveBg", "Remove the background, and output a PNG image"],
    ["qSnow", "Change the scene to a snowy winter night with warm lights in the windows. Keep everything else the same."],
    ["qWatercolor", "Turn this photo into a hand-painted watercolor illustration. Keep the composition."],
    ["qGolden", "Relight this photo as a golden-hour scene with soft warm sunlight. Keep the subject and composition identical."],
    ["qNoPeople", "Remove all the people from the photo and fill the background naturally. Keep everything else identical."],
    ["qCombine", "Place the subject from <image2> into the scene of <image1>, matching lighting and perspective, with a realistic contact shadow."],
  ];
  const STAGE_KEY = { upload: "stUpload", load: "stLoad", encode: "stEncode", sample: "stSample", decode: "stDecode", save: "stSave" };
  const PRESET_KEY = { draft: "pDraft", standard: "pStandard", max: "pMax" };

  const state = {
    mode: "text",
    preset: localGet("preset", "standard"),
    aspect: localGet("aspect", "1:1"),
    surpriseAspect: "random",
    refs: [],
    engine: "off",
    engineExternal: false,
    busy: false,
    current: null,
    history: [],
    sessionOutputs: new Set(),
    generationRun: 0,
    stopRequested: false,
    lastIdea: null,
    stepTimes: [],
    setup: null, // SetupInfo from Rust
    installing: false,
    stepState: {}, // id -> {state, detail, done, total, bps}
  };

  function localGet(k, d) { try { return localStorage.getItem("qil." + k) ?? d; } catch { return d; } }
  function localSet(k, v) { try { localStorage.setItem("qil." + k, v); } catch { /* no storage */ } }
  const fmtBytes = (b) => (b >= 1073741824 ? (b / 1073741824).toFixed(1) + " GB" : Math.round(b / 1048576) + " MB");
  const fmtSecs = (s) => (s >= 60 ? `${Math.floor(s / 60)} min ${String(s % 60).padStart(2, "0")} s` : `${s} s`);

  // ═════════ Setup (first run) ═════════

  const STEP_LABEL = { uv: "stepUv", comfy: "stepComfy", python: "stepPython", deps: "stepDeps" };

  async function loadSetup() {
    state.setup = await invoke("setup_status");
    renderSetup();
    return state.setup;
  }

  function renderSetup() {
    const s = state.setup;
    if (!s) return;
    const g = s.status.gpu;
    $("gpuCard").innerHTML = g
      ? `<strong>${g.name.replace("NVIDIA GeForce ", "")}</strong><span class="mono">${(g.totalMb / 1024).toFixed(1)} GB VRAM · ${t("driver")} ${g.driver}</span>`
      : `<strong>${t("noGpu")}</strong>`;
    const probs = s.status.problems;
    $("problems").hidden = !probs.length;
    $("problems").innerHTML = probs.map((p) => `<p>${escapeHtml(p)}</p>`).join("");

    document.querySelectorAll("#profiles [data-profile]").forEach((b) =>
      b.setAttribute("aria-checked", String(b.dataset.profile === s.config.profile)));
    document.querySelectorAll("[data-rec]").forEach((el) => { el.hidden = el.dataset.rec !== s.recommended; });
    $("modelsDir").textContent = s.config.modelsDir;
    $("modelsDir").title = s.config.modelsDir;

    const dl = s.status.downloadBytes;
    const free = s.status.freeBytes;
    $("summary").textContent = (dl ? `${fmtBytes(dl)} ${t("toDownload")}` : t("nothingToDownload")) + (free != null ? ` · ${fmtBytes(free)} ${t("free")}` : "");

    const steps = $("steps");
    steps.innerHTML = "";
    for (const st of s.status.steps) {
      const live = state.stepState[st.id] || {};
      const isModel = st.id.startsWith("model:");
      const name = isModel ? st.id.slice(6) : t(STEP_LABEL[st.id] || st.id);
      let meta;
      let cls = "";
      if (live.state === "error") { meta = t("failed"); cls = "err"; }
      else if (live.state === "done" || live.state === "skipped" || (st.done && !live.state)) { meta = live.state === "skipped" || st.done ? t("skipped") : t("done"); cls = "ok"; }
      else if (live.state === "running") {
        cls = "pend";
        if (live.total) {
          const pct = Math.floor((live.done / live.total) * 100);
          meta = live.detail === "verifying checksum" ? `${t("verifying")} ${pct}%` : `${pct}% · ${fmtBytes(live.done)} / ${fmtBytes(live.total)}` + (live.bps ? ` · ${fmtBytes(live.bps)}/s` : "");
        } else meta = live.detail || "…";
      } else meta = st.downloadBytes ? fmtBytes(st.downloadBytes) : t("waiting");
      if (live.state === "done") { meta = t("done"); cls = "ok"; }
      const pct = live.state === "running" && live.total ? (live.done / live.total) * 100 : live.state === "done" || live.state === "skipped" || st.done ? 100 : 0;
      const row = document.createElement("div");
      row.className = "srow";
      row.innerHTML = `<span class="dot ${cls}"></span><span class="s-name mono">${escapeHtml(name)}</span><span class="s-meta mono">${escapeHtml(meta)}</span><span class="s-bar"><i style="width:${pct}%"></i></span>`;
      steps.appendChild(row);
    }
    const blocked = probs.some((p) => /NVIDIA|driver|disk|VRAM/i.test(p));
    const btn = $("installBtn");
    if (state.installing) { btn.textContent = t("installing"); btn.disabled = true; }
    else if (s.status.ready) { btn.textContent = t("startApp"); btn.disabled = false; }
    else { btn.textContent = state.failed ? t("retry") : t("install", dl ? fmtBytes(dl) : "0 MB"); btn.disabled = blocked; }
    document.querySelectorAll("#profiles button, #modelsPick").forEach((b) => { b.disabled = state.installing; });
  }

  document.querySelectorAll("#profiles [data-profile]").forEach((b) =>
    b.addEventListener("click", async () => {
      await invoke("setup_configure", { profile: b.dataset.profile, modelsDir: null });
      loadSetup();
    }));
  $("modelsPick").addEventListener("click", async () => {
    const dir = await invoke("pick_folder");
    if (!dir) return;
    await invoke("setup_configure", { profile: state.setup.config.profile, modelsDir: dir });
    loadSetup();
  });
  $("installBtn").addEventListener("click", async () => {
    if (state.setup?.status.ready) return enterApp();
    state.installing = true;
    state.failed = false;
    state.stepState = {};
    $("log").textContent = "";
    renderSetup();
    try { await invoke("setup_run"); } catch (e) { state.installing = false; notifySetup(String(e)); renderSetup(); }
  });

  function onSetupEvent(ev) {
    if (ev.kind === "step") {
      const cur = state.stepState[ev.id] || {};
      state.stepState[ev.id] = { ...cur, state: ev.state, detail: ev.detail };
      if (ev.detail === "verifying checksum") state.stepState[ev.id].total = 0;
      appendLog(`${ev.state === "done" ? "✓" : ev.state === "skipped" ? "·" : ev.state === "error" ? "✗" : "…"} ${ev.id} ${ev.detail || ""}`);
    } else if (ev.kind === "progress") {
      const cur = state.stepState[ev.id] || { state: "running" };
      state.stepState[ev.id] = { ...cur, done: ev.done, total: ev.total, bps: ev.bytesPerSec };
    } else if (ev.kind === "log") {
      appendLog(ev.line);
    }
    throttledRender();
  }
  let renderPending = false;
  function throttledRender() {
    if (renderPending) return;
    renderPending = true;
    requestAnimationFrame(() => { renderPending = false; renderSetup(); });
  }
  function appendLog(line) {
    const log = $("log");
    log.textContent += line + "\n";
    if (log.textContent.length > 60000) log.textContent = log.textContent.slice(-40000);
    log.scrollTop = log.scrollHeight;
  }
  async function onSetupFinished(err) {
    state.installing = false;
    if (err) {
      state.failed = true;
      appendLog("✗ " + err);
      $("logBox").open = true;
      notifySetup(err);
    }
    await loadSetup();
  }
  function notifySetup(msg) {
    $("problems").hidden = false;
    $("problems").insertAdjacentHTML("afterbegin", `<p>${escapeHtml(msg)}</p>`);
  }
  function escapeHtml(s) { return String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]); }

  // ═════════ Main app ═════════

  function enterApp() {
    $("setupView").hidden = true;
    $("mainView").hidden = false;
    ["engineStat", "engineBtn", "folderBtn"].forEach((id) => { $(id).hidden = false; });
    $("settingsBtn").hidden = false;
    const p = state.setup?.config.profile === "compact" ? "w4a8" : "int8";
    $("profileTag").textContent = p;
    invoke("output_dir").then((d) => { $("outDirTxt").textContent = d; }).catch(() => {});
    loadHistory();
    startEngine();
  }

  // Mode
  function setMode(mode) {
    state.mode = mode;
    document.querySelectorAll("[data-mode]").forEach((b) => b.setAttribute("aria-selected", String(b.dataset.mode === mode)));
    document.querySelectorAll("[data-show]").forEach((el) => { el.hidden = !el.dataset.show.split(" ").includes(mode); });
    document.querySelectorAll("[data-only]").forEach((el) => { el.hidden = el.dataset.only !== mode; });
    renderModeHint();
    renderPresets();
    renderAspects();
    updateGenButton();
  }
  function renderModeHint() { $("modeHint").textContent = t({ text: "hintText", photos: "hintPhotos", surprise: "hintSurprise" }[state.mode]); }
  document.querySelectorAll("[data-mode]").forEach((b) => b.addEventListener("click", () => setMode(b.dataset.mode)));

  // Quality
  function renderPresets() {
    document.querySelectorAll("#presets [data-preset]").forEach((b) => {
      b.setAttribute("aria-checked", String(b.dataset.preset === state.preset));
      const meta = b.querySelector(".p-meta");
      meta.textContent = state.mode === "photos" ? meta.dataset.e : meta.dataset.t;
    });
  }
  document.querySelectorAll("#presets [data-preset]").forEach((b) =>
    b.addEventListener("click", () => { state.preset = b.dataset.preset; localSet("preset", state.preset); renderPresets(); updateGenButton(); }));

  // Format
  function renderAspects() {
    const a = state.mode === "surprise" ? state.surpriseAspect : state.aspect;
    document.querySelectorAll("#aspects [data-ar]").forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.ar === a)));
  }
  document.querySelectorAll("#aspects [data-ar]").forEach((b) =>
    b.addEventListener("click", () => {
      if (state.mode === "surprise") state.surpriseAspect = b.dataset.ar;
      else { state.aspect = b.dataset.ar; localSet("aspect", state.aspect); }
      renderAspects();
    }));

  // Seed
  $("seedLock").addEventListener("change", (e) => {
    $("seed").disabled = !e.target.checked;
    if (e.target.checked && !$("seed").value && state.current?.meta?.seed != null) $("seed").value = state.current.meta.seed;
    if (e.target.checked) $("seed").focus();
  });
  $("seed").addEventListener("input", (e) => { e.target.value = e.target.value.replace(/\D/g, "").slice(0, 15); });

  // References
  async function addRefs(paths) {
    const room = MAX_REFS - state.refs.length;
    const fresh = paths.filter((p) => /\.(png|jpe?g|webp|bmp)$/i.test(p) && !state.refs.some((r) => r.path === p || r.orig === p));
    if (fresh.length > room) notify(t("limitRefs", room, fresh.length, MAX_REFS));
    for (const path of fresh.slice(0, room)) {
      const ref = { path, thumb: "" };
      state.refs.push(ref);
      renderRefs();
      try { ref.thumb = await invoke("thumbnail", { path, size: 240 }); } catch (e) { notify(String(e)); }
      renderRefs();
    }
    updateGenButton();
  }
  // Turns a copy of the photo, never the user's file; the copy is what the engine gets.
  async function rotateRef(ref, btn) {
    btn.disabled = true;
    try {
      const path = await invoke("rotate_image", { path: ref.path });
      const thumb = await invoke("thumbnail", { path, size: 240 });
      ref.orig ??= ref.path;
      ref.path = path;
      ref.thumb = thumb;
    } catch (e) { notify(String(e)); }
    renderRefs();
  }
  function renderRefs() {
    const box = $("refs");
    box.innerHTML = "";
    box.classList.toggle("empty", state.refs.length === 0);
    state.refs.forEach((r, i) => {
      const el = document.createElement("div");
      el.className = "ref" + (i === 0 ? " canvas" : "");
      el.title = (i === 0 ? t("canvas") + " · " : "") + r.path;
      el.innerHTML = `${r.thumb ? `<img src="${r.thumb}" alt="">` : ""}<span class="n">${i + 1}</span><button type="button" class="rot" aria-label="${t("rotate")}" title="${t("rotate")}">↻</button><button type="button" class="x" aria-label="${t("remove")}">×</button>`;
      el.querySelector(".x").addEventListener("click", () => { state.refs.splice(i, 1); renderRefs(); updateGenButton(); });
      el.querySelector(".rot").addEventListener("click", (e) => rotateRef(r, e.currentTarget));
      box.appendChild(el);
    });
    if (state.refs.length < MAX_REFS) {
      const add = document.createElement("button");
      add.type = "button";
      add.className = "ref-add";
      add.innerHTML = `<b>+</b>${state.refs.length ? t("add") : t("dropOrClick")}`;
      add.addEventListener("click", async () => addRefs(await invoke("pick_images")));
      box.appendChild(add);
    }
    $("refCount").textContent = `${state.refs.length} / ${MAX_REFS}`;
  }
  function renderQuick() {
    const q = $("quick");
    q.innerHTML = "";
    for (const [key, prompt] of QUICK) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "btn btn-xs";
      b.textContent = t(key);
      b.addEventListener("click", () => { $("promptPhotos").value = prompt; updateGenButton(); });
      q.appendChild(b);
    }
  }

  // Generate button: the label says what will happen
  function currentPrompt() {
    if (state.mode === "text") return $("promptText").value.trim();
    if (state.mode === "photos") return $("promptPhotos").value.trim();
    return "surprise";
  }
  function setGenerateMenu(open) {
    const menu = $("genMenu");
    menu.hidden = !open;
    $("genMenuBtn").setAttribute("aria-expanded", String(open));
  }
  function updateGenButton() {
    const btn = $("genBtn");
    let label = t("gen");
    let ok = true;
    if (state.busy) { label = t("generating"); ok = false; }
    else if (state.engine === "starting") { label = t("starting"); ok = false; }
    else if (state.engine !== "ready") { label = t("engineOffBtn"); ok = false; }
    else if (state.mode === "photos" && state.refs.length === 0) { label = t("needPhoto"); ok = false; }
    else if (!currentPrompt()) { label = state.mode === "photos" ? t("needInstr") : t("needDesc"); ok = false; }
    else if (state.mode === "surprise") label = t("genSurprise");
    else if (state.mode === "photos") label = state.refs.length > 1 ? t("genWith", state.refs.length) : t("genEdit");
    btn.textContent = label;
    btn.disabled = !ok;
    $("genMenuBtn").disabled = !ok;
    $("genForeverBtn").disabled = !ok;
    if (!ok) setGenerateMenu(false);
    $("cancelBtn").hidden = !state.busy;
    $("clearHistoryBtn").disabled = state.busy || state.history.length === 0;
    $("settingsBtn").disabled = state.busy;
    const locked = state.busy;
    document.querySelectorAll("[data-mode], #promptText, #promptPhotos, #theme, #transparent, #seedLock, #seed, #presets [data-preset], #aspects [data-ar], #quick button").forEach((el) => { el.disabled = locked; });
    document.querySelectorAll(".ref-add, .ref .rot, .ref .x").forEach((el) => { el.disabled = locked; });
  }
  ["promptText", "promptPhotos"].forEach((id) => $(id).addEventListener("input", updateGenButton));
  $("genMenuBtn").addEventListener("click", () => setGenerateMenu($("genMenu").hidden));
  $("genForeverBtn").addEventListener("click", () => { setGenerateMenu(false); generate(true); });
  document.addEventListener("click", (e) => { if (!e.target.closest(".gen-split")) setGenerateMenu(false); });

  // Engine
  function renderEngine(ev) {
    if (ev) { state.engine = ev.state; state.engineExternal = !!ev.external; state.engineDetail = ev.detail || ""; }
    $("engineDot").className = "dot " + ({ ready: "ok", starting: "pend", error: "err" }[state.engine] || "");
    $("engineTxt").textContent = t({
      ready: state.engineExternal ? "engineReadyExt" : "engineReady",
      starting: "engineStarting", error: "engineError", off: "engineOff",
    }[state.engine] || "engineOff");
    $("engineStat").title = state.engineDetail || "";
    const eb = $("engineBtn");
    eb.textContent = state.engine === "ready" ? t("stopEngine") : state.engine === "starting" ? t("starting") : t("startEngine");
    eb.disabled = state.engine === "starting" || (state.engine === "ready" && state.engineExternal) || state.busy;
    eb.title = state.engine === "ready" && state.engineExternal ? t("externalTip") : "";
    if (ev && ev.state === "error" && ev.detail) notify(ev.detail);
    updateGenButton();
  }
  async function startEngine() {
    renderEngine({ state: "starting", detail: "" });
    try { await invoke("engine_start"); } catch (e) { renderEngine({ state: "error", detail: String(e) }); }
  }
  $("engineBtn").addEventListener("click", async () => {
    if (state.engine === "ready") await invoke("engine_stop");
    else startEngine();
  });

  // GPU meter
  async function pollGpu() {
    const g = await invoke("gpu_stats").catch(() => null);
    if (!g) { $("gpuStat").hidden = true; return; }
    $("gpuName").textContent = g.name.replace("NVIDIA GeForce ", "").replace("NVIDIA ", "");
    const pct = Math.round((g.usedMb / g.totalMb) * 100);
    $("vramFill").style.width = pct + "%";
    $("vramFill").classList.toggle("high", pct > 92);
    $("vramTxt").textContent = `${(g.usedMb / 1024).toFixed(1)} / ${(g.totalMb / 1024).toFixed(1)} GB`;
    $("gpuStat").title = `VRAM ${pct}% · GPU ${g.util}%`;
  }

  // Generate
  async function generate(continuous = false) {
    if ($("genBtn").disabled) return;
    setGenerateMenu(false);
    const snapshot = {
      mode: state.mode,
      prompt: currentPrompt(),
      images: state.mode === "photos" ? state.refs.map((r) => r.path) : [],
      preset: state.preset,
      aspect: state.aspect,
      surpriseAspect: state.surpriseAspect,
      theme: $("theme").value || null,
      transparent: state.mode === "text" && $("transparent").checked,
      lockedSeed: $("seedLock").checked && $("seed").value ? Number($("seed").value) : null,
      seedLocked: $("seedLock").checked && !!$("seed").value,
    };
    const runId = ++state.generationRun;
    state.stopRequested = false;
    state.busy = true;
    hideNotice();
    updateGenButton();
    renderEngine();

    async function nextRequest() {
      let prompt = snapshot.prompt;
      let aspect = snapshot.aspect;
      if (snapshot.mode === "surprise") {
        const idea = await invoke("surprise_idea", { theme: snapshot.theme });
        prompt = idea.prompt;
        aspect = snapshot.surpriseAspect === "random" ? idea.aspect : snapshot.surpriseAspect;
        state.lastIdea = prompt;
        $("idea").hidden = false;
        $("ideaTxt").textContent = prompt;
      }
      return {
        mode: snapshot.mode,
        prompt,
        images: snapshot.images,
        preset: snapshot.preset,
        aspect,
        seed: snapshot.seedLocked ? snapshot.lockedSeed : null,
        transparent: snapshot.transparent,
      };
    }

    const appendResult = (r, req) => {
      const item = { path: r.path, meta: { ...req, seed: r.seed, width: r.width, height: r.height, seconds: r.seconds, references: req.images } };
      state.sessionOutputs.add(item.path);
      state.history.unshift(item);
      showImage(item);
      renderHistory();
    };

    try {
      let iterations = 0;
      while (continuous || iterations === 0) {
        if (runId !== state.generationRun || state.stopRequested) break;
        const req = await nextRequest();
        if (runId !== state.generationRun || state.stopRequested) break;
        state.stepTimes = [];
        showWorking("load", 0, 0);
        const r = await invoke("generate", { req });
        if (runId !== state.generationRun || state.stopRequested) break;
        appendResult(r, req);
        iterations += 1;
        if (!continuous) break;
      }
      if (state.stopRequested || runId !== state.generationRun) notify(t("cancelled"), true);
    } catch (e) {
      const msg = String(e);
      if (msg === "Cancelled") notify(t("cancelled"), true); else notify(msg);
    } finally {
      state.busy = false;
      state.stopRequested = false;
      hideWorking();
      updateGenButton();
      renderHistory();
      renderEngine();
    }
  }
  $("genBtn").addEventListener("click", () => generate());
  function cancelGeneration() {
    if (!state.busy) return;
    state.generationRun += 1;
    state.stopRequested = true;
    $("cancelBtn").disabled = true;
    invoke("cancel").catch(() => {});
  }
  $("cancelBtn").addEventListener("click", cancelGeneration);

  function showWorking(stage, value, max) {
    $("working").hidden = false;
    $("shot").classList.add("dim");
    $("empty").hidden = true;
    $("wStage").textContent = t(STAGE_KEY[stage] || "stSample");
    const bar = $("progress");
    bar.classList.add("visible");
    if (stage === "sample" && max > 0) {
      bar.classList.remove("indeterminate");
      $("progressFill").style.width = Math.round((value / max) * 100) + "%";
      state.stepTimes.push(performance.now());
      const ts = state.stepTimes.slice(-6);
      const spi = ts.length > 1 ? (ts[ts.length - 1] - ts[0]) / (ts.length - 1) / 1000 : 0;
      const left = spi ? Math.max(0, Math.round((max - value) * spi)) : null;
      $("wStep").textContent = t("step", value, max) + (spi ? ` · ${spi.toFixed(1)} ${t("perStep")} · ${fmtSecs(left)} ${t("left")}` : "");
    } else {
      bar.classList.add("indeterminate");
      $("progressFill").style.width = "";
      $("wStep").textContent = stage === "load" ? t("firstSlow") : "";
    }
  }
  function hideWorking() {
    $("working").hidden = true;
    $("shot").classList.remove("dim");
    $("progress").classList.remove("visible", "indeterminate");
    $("progressFill").style.width = "0";
    $("cancelBtn").disabled = false;
    if ($("shot").hidden) $("empty").hidden = false;
  }

  // Viewer and history
  function showImage(item) {
    state.current = item;
    const img = $("shot");
    img.src = fileSrc(item.path);
    img.hidden = false;
    $("empty").hidden = true;
    const m = item.meta || {};
    $("info").hidden = false;
    $("infoPrompt").textContent = m.prompt || item.path.split(/[\\/]/).pop();
    $("infoPrompt").title = m.prompt || "";
    const parts = [];
    if (m.width) parts.push(`${m.width}×${m.height}`);
    if (m.seed != null) parts.push(`seed ${m.seed}`);
    if (m.preset) parts.push(t(PRESET_KEY[m.preset] || m.preset));
    if (m.seconds) parts.push(fmtSecs(Math.round(m.seconds)));
    if (m.mode === "photos" && m.references?.length) parts.push(`${m.references.length} ${t("refs")}`);
    $("infoMeta").textContent = parts.join(" · ");
    $("aSeed").disabled = m.seed == null;
    $("aCopy").disabled = !m.prompt;
    document.querySelectorAll(".hist").forEach((h) => h.setAttribute("aria-current", String(h.dataset.path === item.path)));
  }
  function renderHistory() {
    const strip = $("history");
    strip.innerHTML = "";
    strip.dataset.empty = t("historyEmpty");
    state.history.slice(0, 60).forEach((item) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "hist";
      b.dataset.path = item.path;
      b.title = item.meta?.prompt || "";
      b.setAttribute("aria-current", String(state.current?.path === item.path));
      b.innerHTML = `<img loading="lazy" decoding="async" src="${fileSrc(item.path)}" alt="">`;
      b.addEventListener("click", () => showImage(item));
      strip.appendChild(b);
    });
    $("histCount").textContent = String(state.history.length);
    $("clearHistoryBtn").disabled = state.busy || state.history.length === 0;
  }
  function clearCurrentPreview() {
    state.current = null;
    $("shot").hidden = true;
    $("shot").removeAttribute("src");
    $("info").hidden = true;
    if (!state.busy) $("empty").hidden = false;
  }
  async function loadHistory() {
    try {
      const outputs = await invoke("list_outputs", { limit: 60 });
      state.history = outputs.filter((item) => state.sessionOutputs.has(item.path));
      renderHistory();
      if (state.history[0]) showImage(state.history[0]);
      else if (state.current && !state.history.some((item) => item.path === state.current.path)) clearCurrentPreview();
    } catch (e) { notify(String(e)); }
  }
  $("clearHistoryBtn").addEventListener("click", () => {
    if (state.busy) return;
    state.sessionOutputs.clear();
    state.history = [];
    clearCurrentPreview();
    renderHistory();
  });

  $("aCopy").addEventListener("click", async () => {
    const p = state.current?.meta?.prompt;
    if (!p) return;
    try { await navigator.clipboard.writeText(p); notify(t("copied"), true); } catch { notify(t("copyFail")); }
  });
  $("aSeed").addEventListener("click", () => {
    const s = state.current?.meta?.seed;
    if (s == null) return;
    $("seedLock").checked = true;
    $("seed").disabled = false;
    $("seed").value = s;
  });
  $("aEdit").addEventListener("click", async () => {
    if (!state.current) return;
    setMode("photos");
    state.refs = state.refs.filter((r) => r.path !== state.current.path);
    state.refs.unshift({ path: state.current.path, thumb: "" });
    state.refs = state.refs.slice(0, MAX_REFS);
    renderRefs();
    state.refs[0].thumb = await invoke("thumbnail", { path: state.refs[0].path, size: 240 }).catch(() => "");
    renderRefs();
    $("promptPhotos").focus();
    updateGenButton();
  });
  $("aReveal").addEventListener("click", () => state.current && invoke("reveal", { path: state.current.path }));
  $("aOpen").addEventListener("click", () => state.current && invoke("open_path", { path: state.current.path }));
  $("folderBtn").addEventListener("click", async () => invoke("open_path", { path: await invoke("output_dir") }));
  let settingsDraft = null;
  function closeSettings() {
    if ($("settingsBackdrop").hidden) return;
    $("settingsBackdrop").hidden = true;
    settingsDraft = null;
    $("settingsBtn").focus();
  }
  async function openSettings() {
    if (state.busy) return;
    try {
      const current = await invoke("app_settings");
      settingsDraft = { outputDir: current.outputDir, saveJson: current.saveJson };
      $("settingsPath").textContent = settingsDraft.outputDir;
      $("settingsPath").title = settingsDraft.outputDir;
      $("saveJson").checked = settingsDraft.saveJson;
      $("settingsError").hidden = true;
      $("settingsBackdrop").hidden = false;
      $("outputPick").focus();
    } catch (error) {
      notify(`${t("settingsSaveFailed")}${String(error)}`);
    }
  }
  function showSettingsError(message) {
    let text = message;
    if (message.includes("generation is running")) text = t("settingsBusy");
    else if (message.includes("Output folder cannot be empty") || message.includes("must be an absolute path")) text = t("invalidFolder");
    $("settingsError").textContent = text;
    $("settingsError").hidden = false;
  }
  $("settingsBtn").addEventListener("click", openSettings);
  $("settingsCancel").addEventListener("click", closeSettings);
  $("settingsBackdrop").addEventListener("click", (event) => { if (event.target === $("settingsBackdrop")) closeSettings(); });
  $("outputPick").addEventListener("click", async () => {
    const selected = await invoke("pick_output_folder").catch((error) => { showSettingsError(String(error)); return null; });
    if (!selected || !settingsDraft) return;
    settingsDraft.outputDir = selected;
    $("settingsPath").textContent = selected;
    $("settingsPath").title = selected;
    $("settingsError").hidden = true;
  });
  $("settingsSave").addEventListener("click", async () => {
    if (!settingsDraft) return;
    if (!settingsDraft.outputDir.trim()) { showSettingsError(t("invalidFolder")); return; }
    settingsDraft.saveJson = $("saveJson").checked;
    $("settingsSave").disabled = true;
    try {
      await invoke("save_app_settings", { outputDir: settingsDraft.outputDir, saveJson: settingsDraft.saveJson });
      closeSettings();
      $("outDirTxt").textContent = await invoke("output_dir");
      await loadHistory();
    } catch (error) {
      const message = String(error);
      showSettingsError(message.includes("generation is running") ? message : `${t("settingsSaveFailed")}${message}`);
    } finally {
      $("settingsSave").disabled = false;
    }
  });
  $("ideaToText").addEventListener("click", () => {
    if (!state.lastIdea) return;
    $("promptText").value = state.lastIdea;
    setMode("text");
  });

  // Zoom
  $("shot").addEventListener("click", () => {
    if (!state.current || state.busy) return;
    const z = document.createElement("div");
    z.className = "zoom";
    z.innerHTML = `<img src="${fileSrc(state.current.path)}" alt="">`;
    z.addEventListener("click", () => z.remove());
    document.body.appendChild(z);
  });

  // Notices
  function notify(msg, ok = false) {
    $("noticeTxt").textContent = msg;
    $("notice").classList.toggle("ok", ok);
    $("notice").hidden = false;
    if (ok) setTimeout(hideNotice, 2600);
  }
  function hideNotice() { $("notice").hidden = true; }
  $("noticeClose").addEventListener("click", hideNotice);

  // Keyboard
  document.addEventListener("keydown", (e) => {
    if (!$("settingsBackdrop").hidden && e.key === "Tab") {
      const controls = [...$("settingsDialog").querySelectorAll("button:not(:disabled), input:not(:disabled)")];
      const first = controls[0], last = controls[controls.length - 1];
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
      else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
      return;
    }
    if (!$("settingsBackdrop").hidden && e.key === "Escape") { e.preventDefault(); closeSettings(); return; }
    if (!$("settingsBackdrop").hidden) return;
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && !$("mainView").hidden) { e.preventDefault(); generate(); }
    if (e.key === "Escape") {
      const z = document.querySelector(".zoom");
      if (z) z.remove();
      else if (state.busy) cancelGeneration();
      else setGenerateMenu(false);
    }
  });

  // Language
  const langToggle = $("langToggle");
  const langMenu = $("languageMenu");
  const closeLangMenu = () => { langMenu.hidden = true; langToggle.setAttribute("aria-expanded", "false"); };
  langToggle.addEventListener("click", () => {
    langMenu.hidden = !langMenu.hidden;
    langToggle.setAttribute("aria-expanded", String(!langMenu.hidden));
  });
  document.querySelectorAll("[data-lang]").forEach((b) => b.addEventListener("click", () => {
    window.i18n.set(b.dataset.lang);
    closeLangMenu();
  }));
  document.addEventListener("click", (e) => { if (!e.target.closest(".lang")) closeLangMenu(); });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") closeLangMenu(); });
  window.addEventListener("langchange", () => {
    renderModeHint();
    renderQuick();
    renderRefs();
    renderHistory();
    renderEngine();
    renderSetup();
    if (state.current) showImage(state.current);
  });

  // Tauri events (drag & drop events carry real file paths)
  async function wireEvents() {
    try {
      await listen("engine", (e) => renderEngine(e.payload));
      await listen("gen-progress", (e) => { if (state.busy) showWorking(e.payload.stage, e.payload.value, e.payload.max); });
      await listen("setup", (e) => onSetupEvent(e.payload));
      await listen("setup-finished", (e) => onSetupFinished(e.payload));
      await listen("tauri://drag-enter", () => { if (!$("mainView").hidden) $("dropveil").hidden = false; });
      await listen("tauri://drag-leave", () => { $("dropveil").hidden = true; });
      await listen("tauri://drag-drop", (e) => {
        $("dropveil").hidden = true;
        const paths = e.payload?.paths || [];
        if (!paths.length || $("mainView").hidden) return;
        if (state.mode !== "photos") setMode("photos");
        addRefs(paths);
      });
    } catch (e) {
      notify(t("eventsFail") + e);
    }
  }

  // Boot
  (async () => {
    window.i18n.apply();
    setMode("text");
    renderQuick();
    renderRefs();
    await wireEvents();
    pollGpu();
    setInterval(pollGpu, 2500);
    const s = await loadSetup().catch((e) => { notifySetup(String(e)); return null; });
    if (s && s.status.ready && s.configured) enterApp();
    else $("setupView").hidden = false;
  })();
})();
