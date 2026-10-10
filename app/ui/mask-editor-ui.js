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

    let image = null;
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
      ctx.globalAlpha = OVERLAY_ALPHA;
      ctx.drawImage(overlay, 0, 0);
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
      requestDraw();
      renderControls();
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
      last = p;
      drawSegment(p, p);
      requestDraw();
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
      const locked = generating || !open;
      $("maskBrush").setAttribute("aria-pressed", String(model.tool === "brush"));
      $("maskEraser").setAttribute("aria-pressed", String(model.tool === "eraser"));
      $("maskSizeValue").textContent = `${model.brushSize} px`;
      $("maskFeatherValue").textContent = `${$("maskFeather").value} px`;
      for (const id of ["maskBrush", "maskEraser", "maskUndoBtn", "maskRedoBtn", "maskClearBtn", "maskFitBtn", "maskSize", "maskFeather", "maskInstruction", "maskRef"]) {
        $(id).disabled = locked;
      }
      $("maskGenerate").disabled = locked;
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
      image = null;
      generating = false;
      hover = null;
      showError("");
      $("maskDiscard").hidden = true;
      $("maskBackdrop").hidden = false;
      $("maskInstruction").value = deps.getPrompt();
      autoText = "";
      $("maskRef").value = "";
      renderReferences();
      $("maskLoading").hidden = false;
      model.close();
      renderControls();
      observer ??= new ResizeObserver(resize);
      observer.observe($("maskStage"));
      resize();
      $("maskDialog").focus();
      const img = new Image();
      img.onload = () => {
        if (!isOpen() || sourcePath !== path) return;
        image = img;
        const width = img.naturalWidth;
        const height = img.naturalHeight;
        model.open({ source: path, width, height });
        overlay.width = width;
        overlay.height = height;
        model.setTool("brush");
        $("maskSize").max = String(Math.min(512, Math.max(64, Math.round(Math.max(width, height) / 6))));
        setBrush(Math.max(4, Math.round(Math.max(width, height) / 25)));
        fit();
        syncOverlay();
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
      $("maskFeather").addEventListener("input", renderControls);
      $("maskRef").addEventListener("change", onReferenceChange);
      $("maskUndoBtn").addEventListener("click", () => { if (model.undo()) syncOverlay(); });
      $("maskRedoBtn").addEventListener("click", () => { if (model.redo()) syncOverlay(); });
      $("maskClearBtn").addEventListener("click", () => { if (model.clear()) syncOverlay(); });
      $("maskFitBtn").addEventListener("click", fit);
      $("maskGenerate").addEventListener("click", generate);
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
