// Mask editor dialog: canvas view over the QilMaskEditor model.
// The model owns the mask (source-pixel space); this file only draws it, maps
// pointer events through the current view transform, and runs the dialog.
(() => {
  const OVERLAY_ALPHA = 0.5;
  const MAX_ZOOM = 16;

  // deps: { getElement, translate, fileSrc, getSourcePath(), getPrompt(),
  //         getReferences() -> [{path, name}] (photos other than the source),
//         canRun() -> string|null (an error text), submit(payload) -> Promise<{ok}|{error}>,
  //         cancel() }
  function create(deps) {
    const $ = deps.getElement;
    const t = deps.translate;
    const Editor = window.QilMaskEditor;
    const model = Editor.create({ encodePng }); // encodePng is a hoisted function declaration
    const overlay = document.createElement("canvas"); // source-sized, opaque red where masked
    const octx = overlay.getContext("2d");
    const canvas = $("maskCanvas");
    const ctx = canvas.getContext("2d");
    const effectiveOverlay = document.createElement("canvas");
    const effectiveCtx = effectiveOverlay.getContext("2d");
    const rawOutline = document.createElement("canvas");
    const outlineCtx = rawOutline.getContext("2d");

    let image = null;
    let maskHasPixels = false;
    let effectiveReady = false;
    let previewRevision = 0;
    let previewTimer = 0;
    let featherMode = "inward";
    let sourcePath = null;
    let view = { zoom: 1, panX: 0, panY: 0, w: 1, h: 1 };
    let hover = null; // CSS coords of the pointer, for the brush ring
    let stroking = false;
    let panning = null;
    let spaceDown = false;
    let generating = false;
    let frame = 0;
    let opener = null;
    let observer = null;
    let autoText = ""; // instruction text we filled in for a chosen reference
    let lamaStatus = null; // { state: "missing" | "ready" | "invalid" }
    let lamaBusy = false;
    let lamaUnlisten = null;

    const fmtBytes = (bytes) => {
      if (!bytes) return "0 MB";
      const mb = bytes / 1048576;
      return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`;
    };

    const maskPreview = window.QilMaskPreview.create({
      invoke: (args) => deps.invoke("preview_mask_feather", args),
      onBusy(busy) {
        $("maskPreviewStatus").hidden = !busy;
        if (busy) $("maskPreviewStatus").textContent = t("maskPreviewBusy");
      },
      onResult(base64) {
        const result = new Image();
        result.onload = () => {
          if (!isOpen() || !image || result.naturalWidth !== model.size.width || result.naturalHeight !== model.size.height) return;
          effectiveOverlay.width = result.naturalWidth;
          effectiveOverlay.height = result.naturalHeight;
          effectiveCtx.clearRect(0, 0, result.naturalWidth, result.naturalHeight);
          effectiveCtx.drawImage(result, 0, 0);
          const pixels = effectiveCtx.getImageData(0, 0, result.naturalWidth, result.naturalHeight);
          for (let i = 0; i < pixels.data.length; i += 4) {
            const alpha = pixels.data[i];
            pixels.data[i] = 235;
            pixels.data[i + 1] = 48;
            pixels.data[i + 2] = 48;
            pixels.data[i + 3] = Math.round(alpha * OVERLAY_ALPHA);
          }
          effectiveCtx.putImageData(pixels, 0, 0);
          effectiveReady = true;
          $("maskPreviewStatus").hidden = true;
          requestDraw();
        };
        result.src = `data:image/png;base64,${base64}`;
      },
      onError() {
        effectiveReady = false;
        $("maskPreviewStatus").hidden = false;
        $("maskPreviewStatus").textContent = t("maskPreviewFailed");
        requestDraw();
      },
    });

    async function requestPreview() {
      if (!isOpen() || !model.size || generating) return;
      const revision = ++previewRevision;
      const source = sourcePath;
      effectiveReady = false;
      requestDraw();
      try {
        const maskPngBase64 = toBase64(await encodePng(model.rasterize(), model.size.width, model.size.height));
        if (revision !== previewRevision || source !== sourcePath || !isOpen()) return;
        await maskPreview.request({
          maskPngBase64,
          featherPx: Number($("maskFeather").value),
          featherMode,
          revision,
          source,
        });
      } catch {
        if (revision === previewRevision) {
          effectiveReady = false;
          $("maskPreviewStatus").hidden = false;
          $("maskPreviewStatus").textContent = t("maskPreviewFailed");
          requestDraw();
        }
      }
    }

    function schedulePreview() {
      if (previewTimer) clearTimeout(previewTimer);
      previewTimer = setTimeout(() => {
        previewTimer = 0;
        void requestPreview();
      }, 120);
    }

    function updateRawOutline(gray, width, height) {
      rawOutline.width = width;
      rawOutline.height = height;
      const data = outlineCtx.createImageData(width, height);
      for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
          const i = y * width + x;
          if (!gray[i]) continue;
          if (x === 0 || y === 0 || x === width - 1 || y === height - 1 || !gray[i - 1] || !gray[i + 1] || !gray[i - width] || !gray[i + width]) {
            data.data[i * 4] = 235;
            data.data[i * 4 + 1] = 48;
            data.data[i * 4 + 2] = 48;
            data.data[i * 4 + 3] = 230;
          }
        }
      }
      outlineCtx.putImageData(data, 0, 0);
    }

    const isOpen = () => !$("maskBackdrop").hidden;
    const transform = () =>
      Editor.viewTransform({
        width: model.size.width,
        height: model.size.height,
        viewWidth: view.w,
        viewHeight: view.h,
        zoom: view.zoom,
        panX: view.panX,
        panY: view.panY,
      });

    function requestDraw() {
      if (!frame) frame = requestAnimationFrame(draw);
    }

    function draw() {
      frame = 0;
      if (!isOpen() || !image || !model.size) return;
      const dpr = window.devicePixelRatio || 1;
      const backing = Editor.canvasBackingSize(view.w, view.h, dpr);
      if (canvas.width !== backing.width || canvas.height !== backing.height) {
        canvas.width = backing.width;
        canvas.height = backing.height;
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, view.w, view.h);
      const tr = transform();
      ctx.save();
      ctx.translate(tr.offsetX, tr.offsetY);
      ctx.scale(tr.scale, tr.scale);
      ctx.imageSmoothingQuality = "high";
      ctx.drawImage(image, 0, 0);
      if (stroking) {
        ctx.globalAlpha = OVERLAY_ALPHA;
        ctx.drawImage(overlay, 0, 0);
        ctx.globalAlpha = 1;
      }
      if (effectiveReady) ctx.drawImage(effectiveOverlay, 0, 0);
      ctx.drawImage(rawOutline, 0, 0);
      ctx.restore();
      if (hover && !generating) {
        ctx.beginPath();
        ctx.arc(hover.x, hover.y, Math.max(1, (model.brushSize / 2) * tr.scale), 0, Math.PI * 2);
        ctx.lineWidth = 3;
        ctx.strokeStyle = "rgba(0,0,0,.65)";
        ctx.stroke();
        ctx.lineWidth = 1.5;
        ctx.strokeStyle = model.tool === "eraser" ? "#8fd3ff" : "#ffffff";
        ctx.setLineDash(model.tool === "eraser" ? [4, 3] : []);
        ctx.stroke();
        ctx.setLineDash([]);
      }
    }

    // Repaint the overlay from the model after anything but a live stroke.
    function syncOverlay() {
      const { width, height } = model.size;
      const gray = model.rasterize();
      maskHasPixels = Editor.maskHasPixels(gray);
      if (!maskHasPixels) {
        octx.clearRect(0, 0, overlay.width, overlay.height);
        outlineCtx.clearRect(0, 0, rawOutline.width, rawOutline.height);
        effectiveReady = false;
        maskPreview.invalidate(sourcePath);
        $("maskPreviewStatus").hidden = true;
        requestDraw();
        renderControls();
        return;
      }
      if (overlay.width !== width || overlay.height !== height) {
        overlay.width = width;
        overlay.height = height;
      }
      const data = octx.createImageData(width, height);
      for (let i = 0; i < gray.length; i++) {
        if (gray[i]) {
          data.data[i * 4] = 235;
          data.data[i * 4 + 1] = 48;
          data.data[i * 4 + 2] = 48;
          data.data[i * 4 + 3] = 255;
        }
      }
      octx.putImageData(data, 0, 0);
      updateRawOutline(gray, width, height);
      requestDraw();
      renderControls();
      schedulePreview();
    }

    function localPoint(event) {
      const rect = canvas.getBoundingClientRect();
      return { x: event.clientX - rect.left, y: event.clientY - rect.top };
    }

    function clampPoint(p) {
      const { width, height } = model.size;
      return { x: Math.min(width, Math.max(0, p.x)), y: Math.min(height, Math.max(0, p.y)) };
    }

    let last = null;
    function drawSegment(from, to) {
      octx.save();
      octx.globalCompositeOperation = model.tool === "eraser" ? "destination-out" : "source-over";
      octx.strokeStyle = "rgb(235,48,48)";
      octx.fillStyle = "rgb(235,48,48)";
      octx.lineWidth = model.brushSize;
      octx.lineCap = "round";
      octx.beginPath();
      octx.moveTo(from.x, from.y);
      octx.lineTo(to.x, to.y);
      octx.stroke();
      octx.restore();
    }

    function onPointerDown(event) {
      if (!image || generating) return;
      const panButton = event.button === 1 || event.button === 2 || spaceDown;
      canvas.setPointerCapture(event.pointerId);
      if (panButton) {
        event.preventDefault();
        panning = { x: event.clientX, y: event.clientY, panX: view.panX, panY: view.panY };
        return;
      }
      if (event.button !== 0) return;
      const start = localPoint(event);
      const p = clampPoint(Editor.toSource(transform(), start.x, start.y));
      if (!model.beginStroke(p.x, p.y)) return;
      stroking = true;
      if (overlay.width !== model.size.width || overlay.height !== model.size.height) {
        overlay.width = model.size.width;
        overlay.height = model.size.height;
      }
      maskHasPixels = model.tool === "brush" || maskHasPixels;
      last = p;
      drawSegment(p, p);
      requestDraw();
      schedulePreview();
    }

    function onPointerMove(event) {
      hover = localPoint(event);
      if (panning) {
        view.panX = panning.panX + event.clientX - panning.x;
        view.panY = panning.panY + event.clientY - panning.y;
      } else if (stroking) {
        const p = clampPoint(Editor.toSource(transform(), hover.x, hover.y));
        model.addPoint(p.x, p.y);
        drawSegment(last, p);
        last = p;
        schedulePreview();
      }
      requestDraw();
    }

    function endPointer(event) {
      if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
      panning = null;
      if (stroking) {
        stroking = false;
        model.endStroke();
        syncOverlay();
      }
    }

    function onWheel(event) {
      if (!image) return;
      event.preventDefault();
      const p = localPoint(event);
      const before = Editor.toSource(transform(), p.x, p.y);
      const factor = Math.exp(-event.deltaY * 0.0015);
      view.zoom = Math.min(MAX_ZOOM, Math.max(1, view.zoom * factor));
      // Keep the source point under the cursor fixed.
      const next = Editor.viewTransform({
        width: model.size.width,
        height: model.size.height,
        viewWidth: view.w,
        viewHeight: view.h,
        zoom: view.zoom,
      });
      view.panX = p.x - before.x * next.scale - next.offsetX;
      view.panY = p.y - before.y * next.scale - next.offsetY;
      if (view.zoom === 1) view.panX = view.panY = 0;
      requestDraw();
    }

    function fit() {
      view.zoom = 1;
      view.panX = 0;
      view.panY = 0;
      requestDraw();
    }

    function resize() {
      const stage = $("maskStage");
      view.w = Math.max(1, Math.floor(stage.clientWidth));
      view.h = Math.max(1, Math.floor(stage.clientHeight));
      canvas.style.width = view.w + "px";
      canvas.style.height = view.h + "px";
      requestDraw();
    }

    function showError(text) {
      const el = $("maskError");
      el.textContent = text || "";
      el.hidden = !text;
    }

    function renderControls() {
      const size = model.size;
      const open = Boolean(size);
      const ready = lamaStatus?.state === "ready";
      const locked = generating || lamaBusy || !open;
      $("maskBrush").setAttribute("aria-pressed", String(model.tool === "brush"));
      $("maskEraser").setAttribute("aria-pressed", String(model.tool === "eraser"));
      $("maskSizeValue").textContent = `${model.brushSize} px`;
      $("maskFeatherNumber").value = $("maskFeather").value;
      $("maskGaussianWarning").hidden = featherMode !== "bothSides";
      for (const id of ["maskBrush", "maskEraser", "maskUndoBtn", "maskRedoBtn", "maskClearBtn", "maskFitBtn", "maskSize", "maskFeather", "maskFeatherNumber", "maskFeatherMode", "maskDenoisingStrength", "maskDenoisingStrengthNumber", "maskCfg", "maskCfgNumber", "maskInstruction", "maskRef"]) {
        $(id).disabled = locked;
      }
      $("maskGenerate").disabled = locked;
      $("maskErase").hidden = !ready;
      $("maskErase").disabled = locked || !ready;
      $("maskDownload").hidden = !lamaStatus || ready;
      $("maskDownload").disabled = locked || lamaBusy;
      $("maskDownload").textContent = lamaBusy ? t("lamaDownloading") : t("downloadLama");
      $("maskClose").textContent = generating ? t("maskCancelRun") : t("maskClose");
      $("maskGenerate").textContent = generating ? t("generating") : t("maskGenerate");
      $("maskCanvas").setAttribute("aria-busy", String(generating));
    }

    // Offers the other Edit references as content for the painted area.
    function renderReferences() {
      const refs = deps.getReferences();
      const select = $("maskRef");
      const previous = select.value;
      select.innerHTML = "";
      const none = document.createElement("option");
      none.value = "";
      none.textContent = t("maskRefNone");
      select.appendChild(none);
      refs.forEach((ref) => {
        const option = document.createElement("option");
        option.value = ref.path;
        option.textContent = ref.name;
        select.appendChild(option);
      });
      select.value = refs.some((r) => r.path === previous) ? previous : "";
      $("maskRefRow").hidden = refs.length === 0;
    }

    function onReferenceChange() {
      const box = $("maskInstruction");
      const current = box.value.trim();
      if ($("maskRef").value) {
        if (!current || current === autoText) {
          autoText = t("maskRefInstruction");
          box.value = autoText;
        }
      } else if (autoText && current === autoText) {
        box.value = "";
        autoText = "";
      }
    }

    function setBrush(px) {
      model.setBrushSize(px);
      $("maskSize").value = String(model.brushSize);
      renderControls();
      requestDraw();
    }

    async function encodePng(gray, width, height) {
      const c = document.createElement("canvas");
      c.width = width;
      c.height = height;
      const cx = c.getContext("2d");
      const data = cx.createImageData(width, height);
      for (let i = 0; i < gray.length; i++) {
        data.data[i * 4] = data.data[i * 4 + 1] = data.data[i * 4 + 2] = gray[i];
        data.data[i * 4 + 3] = 255;
      }
      cx.putImageData(data, 0, 0);
      const blob = await new Promise((resolve, reject) =>
        c.toBlob((b) => (b ? resolve(b) : reject(new Error("PNG encoding failed"))), "image/png"));
      return new Uint8Array(await blob.arrayBuffer());
    }

    function toBase64(bytes) {
      let out = "";
      for (let i = 0; i < bytes.length; i += 0x8000) {
        out += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
      }
      return btoa(out);
    }

    async function open() {
      if (isOpen()) return;
      const path = deps.getSourcePath();
      if (!path) return;
      opener = document.activeElement;
      sourcePath = path;
      maskHasPixels = false;
      maskPreview.invalidate(path);
      $("maskPreviewStatus").hidden = true;
      previewRevision++;
      effectiveReady = false;
      image = null;
      generating = false;
      lamaStatus = null;
      lamaBusy = false;
      lamaUnlisten?.();
      lamaUnlisten = null;
      setStatus("");
      hover = null;
      showError("");
      $("maskDiscard").hidden = true;
      $("maskBackdrop").hidden = false;
      $("maskInstruction").value = deps.getPrompt();
      const denoisingStrength = Number(deps.getDenoisingStrength?.() ?? 0.75);
      $("maskDenoisingStrength").value = denoisingStrength.toFixed(2);
      $("maskDenoisingStrengthNumber").value = denoisingStrength.toFixed(2);
      const cfg = Number(deps.getCfg?.() ?? 1.0);
      $("maskCfg").value = cfg.toFixed(1);
      $("maskCfgNumber").value = cfg.toFixed(1);
      $("maskCfgNumber").setCustomValidity("");
      autoText = "";
      $("maskRef").value = "";
      renderReferences();
      $("maskLoading").hidden = false;
      model.close();
      // A reopened dialog starts with an empty mask; drop any overlay from the
      // previous photo so it cannot look like a pre-painted selection.
      octx.clearRect(0, 0, overlay.width, overlay.height);
      outlineCtx.clearRect(0, 0, rawOutline.width, rawOutline.height);
      effectiveCtx.clearRect(0, 0, effectiveOverlay.width, effectiveOverlay.height);
      effectiveReady = false;
      renderControls();
      observer ??= new ResizeObserver(resize);
      observer.observe($("maskStage"));
      resize();
      $("maskDialog").focus();
      await refreshLama();
      const img = new Image();
      img.onload = () => {
        if (!isOpen() || sourcePath !== path) return;
        image = img;
        const width = img.naturalWidth;
        const height = img.naturalHeight;
        model.open({ source: path, width, height });
        model.setTool("brush");
        $("maskSize").max = String(Math.min(512, Math.max(64, Math.round(Math.max(width, height) / 6))));
        setBrush(Math.max(4, Math.round(Math.max(width, height) / 25)));
        fit();
        // A newly opened mask is empty: defer source-sized overlay buffers and
        // their full-image raster pass until the user actually paints.
        requestDraw();
        $("maskLoading").hidden = true;
        $("maskBrush").focus();
      };
      img.onerror = () => {
        if (!isOpen()) return;
        deps.notify(t("maskLoadFailed"));
        closeNow();
      };
      img.src = deps.fileSrc(path);
    }

    function closeNow() {
      observer?.disconnect();
      model.close();
      image = null;
      sourcePath = null;
      previewRevision++;
      maskPreview.invalidate(null);
      if (previewTimer) clearTimeout(previewTimer);
      previewTimer = 0;
      lamaUnlisten?.();
      lamaUnlisten = null;
      lamaBusy = false;
      lamaStatus = null;
      effectiveReady = false;
      generating = false;
      $("maskBackdrop").hidden = true;
      if (opener && opener.isConnected) opener.focus();
      opener = null;
    }

    // Closing with an unsaved mask asks first; the mask is never kept for another photo.
    function requestClose() {
      if (generating) {
        deps.cancel();
        return;
      }
      if (model.size && model.isDirty() && !model.isEmpty()) {
        $("maskDiscard").hidden = false;
        $("maskKeep").focus();
        return;
      }
      closeNow();
    }

    async function refreshLama() {
      try {
        const status = await deps.invoke("lama_status");
        if (!isOpen()) return;
        lamaStatus = status;
      } catch (error) {
        if (!isOpen()) return;
        lamaStatus = { state: "invalid" };
        showError(String(error));
      }
      renderControls();
    }

    async function downloadLama() {
      if (lamaBusy || generating) return;
      lamaBusy = true;
      showError("");
      setStatus(t("lamaDownloading"));
      renderControls();
      lamaUnlisten?.();
      lamaUnlisten = null;
      try {
        if (deps.listen) {
          lamaUnlisten = await deps.listen("lama-download-progress", (event) => {
            if (!isOpen()) return;
            const progress = event.payload || {};
            const total = Number(progress.total) || 0;
            const percent = total ? Math.floor((Number(progress.done) / total) * 100) : 0;
            setStatus(
              `${t("lamaDownloading")} ${percent}% · ${fmtBytes(progress.done)} / ${fmtBytes(total)}`,
            );
          });
        }
        await deps.invoke("lama_download");
        const status = await deps.invoke("lama_status");
        if (isOpen()) lamaStatus = status;
        if (status?.state === "ready") {
          setStatus(t("lamaReady"));
        } else {
          setStatus("");
          showError(t("lamaDownloadFailed"));
        }
      } catch (error) {
        if (isOpen()) {
          setStatus("");
          showError(String(error) || t("lamaDownloadFailed"));
        }
      } finally {
        lamaUnlisten?.();
        lamaUnlisten = null;
        lamaBusy = false;
        if (isOpen()) renderControls();
      }
    }

    async function erase() {
      if (generating || lamaBusy || !image) return;
      showError("");
      if (lamaStatus?.state !== "ready") {
        showError(t("downloadLama"));
        return;
      }
      if (model.isEmpty()) {
        showError(t("maskEmpty"));
        return;
      }
      const blocked = deps.canRun();
      if (blocked) {
        showError(blocked);
        return;
      }
      let maskPngBase64;
      try {
        maskPngBase64 = toBase64(await model.getMaskPng());
      } catch (error) {
        showError(String(error.message || error));
        return;
      }
      generating = true;
      model.setBusy(true);
      stroking = false;
      $("maskDiscard").hidden = true;
      setStatus(t("erasing"));
      renderControls();
      let outcome;
      try {
        outcome = await deps.submitErase({ sourcePath, maskPngBase64 });
      } catch (error) {
        outcome = { error: String(error) };
      }
      generating = false;
      model.setBusy(false);
      setStatus("");
      if (!isOpen()) return;
      if (outcome && outcome.ok) {
        closeNow();
        return;
      }
      if (outcome && outcome.error) showError(outcome.error);
      renderControls();
    }

    async function generate() {
      if (generating || !image) return;
      showError("");
      const instruction = $("maskInstruction").value.trim();
      if (!instruction) {
        showError(t("maskNeedInstruction"));
        $("maskInstruction").focus();
        return;
      }
      if (model.isEmpty()) {
        showError(t("maskEmpty"));
        return;
      }
      if (!$("maskDenoisingStrengthNumber").checkValidity()) {
        $("maskDenoisingStrengthNumber").reportValidity();
        return;
      }
      if (!$("maskCfgNumber").checkValidity()) {
        $("maskCfgNumber").reportValidity();
        return;
      }
      const blocked = deps.canRun();
      if (blocked) {
        showError(blocked);
        return;
      }
      let maskPngBase64;
      try {
        maskPngBase64 = toBase64(await model.getMaskPng());
      } catch (error) {
        showError(String(error.message || error));
        return;
      }
      generating = true;
      model.setBusy(true);
      stroking = false;
      $("maskDiscard").hidden = true;
      setStatus(t("stLoad"));
      renderControls();
      let outcome;
      try {
        outcome = await deps.submit({
          sourcePath,
          maskPngBase64,
          instruction,
              featherPx: Number($("maskFeather").value),
              featherMode,
          denoisingStrength: Number($("maskDenoisingStrength").value),
          cfg: Number($("maskCfgNumber").value),
          references: $("maskRef").value ? [$("maskRef").value] : [],
        });
      } catch (error) {
        outcome = { error: String(error) };
      }
      generating = false;
      model.setBusy(false);
      setStatus("");
      if (!isOpen()) return;
      if (outcome && outcome.ok) {
        closeNow();
        return;
      }
      // Failure or cancel: the mask stays so the user can retry.
      if (outcome && outcome.error) showError(outcome.error);
      renderControls();
    }

    function setStatus(text) {
      $("maskStatus").textContent = text;
    }

    function handleKeydown(event) {
      if (!isOpen()) return;
      const typing = event.target.matches?.("textarea, input[type=text]");
      if (event.key === "Escape") {
        event.preventDefault();
        if (!$("maskDiscard").hidden) {
          $("maskDiscard").hidden = true;
          $("maskBrush").focus();
        } else requestClose();
        return;
      }
      if (event.key === "Tab") {
        const controls = [...$("maskDialog").querySelectorAll("button:not(:disabled), input:not(:disabled), textarea:not(:disabled)")]
          .filter((el) => el.offsetParent !== null);
        const first = controls[0];
        const lastControl = controls[controls.length - 1];
        if (event.shiftKey && (document.activeElement === first || document.activeElement === $("maskDialog"))) {
          event.preventDefault();
          lastControl.focus();
        } else if (!event.shiftKey && document.activeElement === lastControl) {
          event.preventDefault();
          first.focus();
        }
        return;
      }
      if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
        event.preventDefault();
        generate();
        return;
      }
      if (event.code === "Space" && !typing && !event.target.matches?.("button")) {
        spaceDown = true;
        event.preventDefault();
        return;
      }
      if (!typing && (event.ctrlKey || event.metaKey) && !generating && model.size) {
        const key = event.key.toLowerCase();
        if (key === "z" && !event.shiftKey) { event.preventDefault(); if (model.undo()) syncOverlay(); }
        else if (key === "y" || (key === "z" && event.shiftKey)) { event.preventDefault(); if (model.redo()) syncOverlay(); }
      }
    }

    function wire() {
      canvas.addEventListener("pointerdown", onPointerDown);
      canvas.addEventListener("pointermove", onPointerMove);
      canvas.addEventListener("pointerup", endPointer);
      canvas.addEventListener("pointercancel", endPointer);
      canvas.addEventListener("pointerleave", () => { hover = null; requestDraw(); });
      canvas.addEventListener("wheel", onWheel, { passive: false });
      canvas.addEventListener("contextmenu", (e) => e.preventDefault());
      document.addEventListener("keyup", (e) => { if (e.code === "Space") spaceDown = false; });
      $("maskBrush").addEventListener("click", () => { model.setTool("brush"); renderControls(); requestDraw(); });
      $("maskEraser").addEventListener("click", () => { model.setTool("eraser"); renderControls(); requestDraw(); });
      $("maskSize").addEventListener("input", (e) => setBrush(Number(e.target.value)));
      $("maskCfg").addEventListener("input", () => {
        $("maskCfgNumber").setCustomValidity("");
        $("maskCfgNumber").value = Number($("maskCfg").value).toFixed(1);
      });
      $("maskCfgNumber").addEventListener("input", () => {
        const field = $("maskCfgNumber");
        field.setCustomValidity("");
        const value = Number(field.value);
        if (field.value === "" || !Number.isFinite(value) || value < 1 || value > 5 || !field.checkValidity()) {
          field.setCustomValidity(t("cfgInvalid"));
          return;
        }
        $("maskCfg").value = value.toFixed(1);
      });
      $("maskFeather").addEventListener("input", () => {
        $("maskFeatherNumber").value = $("maskFeather").value;
        renderControls();
        schedulePreview();
      });
      $("maskFeatherNumber").addEventListener("input", () => {
        const field = $("maskFeatherNumber");
        if (!field.checkValidity() || field.value === "") return;
        $("maskFeather").value = String(Math.round(Number(field.value)));
        renderControls();
        schedulePreview();
      });
      $("maskFeatherMode").addEventListener("change", () => {
        featherMode = $("maskFeatherMode").value === "bothSides" ? "bothSides" : "inward";
        renderControls();
        schedulePreview();
      });
      $("maskRef").addEventListener("change", onReferenceChange);
      $("maskUndoBtn").addEventListener("click", () => { if (model.undo()) syncOverlay(); });
      $("maskRedoBtn").addEventListener("click", () => { if (model.redo()) syncOverlay(); });
      $("maskClearBtn").addEventListener("click", () => { if (model.clear()) syncOverlay(); });
      $("maskFitBtn").addEventListener("click", fit);
      $("maskGenerate").addEventListener("click", generate);
      $("maskErase").addEventListener("click", erase);
      $("maskDownload").addEventListener("click", downloadLama);
      $("maskClose").addEventListener("click", requestClose);
      $("maskDiscardBtn").addEventListener("click", closeNow);
      $("maskKeep").addEventListener("click", () => { $("maskDiscard").hidden = true; $("maskBrush").focus(); });
      window.addEventListener("langchange", () => {
        if (isOpen()) renderReferences();
        renderControls();
      });
    }
    wire();

    return {
      open,
      isOpen,
      handleKeydown,
      setStatus,
      get busy() { return generating; },
    };
  }

  window.QilMaskEditorUi = Object.freeze({ create });
})();
