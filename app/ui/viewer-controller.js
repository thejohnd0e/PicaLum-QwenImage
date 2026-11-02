(() => {
  const clamp = (value, max) => Math.min(Math.max(value, 0), max);

  function create({ state, getElement, fileSrc, translate }) {
    const $ = getElement;
    const images = ["beforeShot", "afterShot", "wipeBefore", "wipeAfter", "holdBefore", "holdAfter"].map((id) => $(id));
    let selectedItem = null;
    let drag = null;
    let holdingPointer = null;
    let holdingElement = null;
    let holdingSpace = false;

    function renderHold() {
      const showingOriginal = holdingPointer !== null || holdingSpace;
      $("holdBefore").hidden = !showingOriginal;
      $("holdAfter").hidden = showingOriginal;
      $("holdLabel").textContent = translate(showingOriginal ? "before" : "after");
    }

    function beginHold(pointerId, element) {
      holdingPointer = pointerId;
      holdingElement = element;
      try {
        element.setPointerCapture?.(pointerId);
      } catch {
        // Capture is a convenience; the window-level release still ends the hold.
      }
      renderHold();
    }

    function releaseHold() {
      const pointerId = holdingPointer;
      const element = holdingElement;
      holdingPointer = null;
      holdingElement = null;
      holdingSpace = false;
      if (pointerId !== null && element?.hasPointerCapture?.(pointerId)) {
        try {
          element.releasePointerCapture(pointerId);
        } catch {
          // The pointer may already be gone.
        }
      }
      renderHold();
    }

    function setMode(mode) {
      releaseHold();
      state.compareMode = mode;
      $("shot").hidden = mode !== "result";
      $("compareView").hidden = mode === "result";
      $("comparePanes").hidden = mode !== "sideBySide";
      $("wipeStage").hidden = mode !== "wipe";
      $("holdStage").hidden = mode !== "hold";
      $("zoomControls").hidden = mode === "result";
      const modes = { viewResult: "result", viewSideBySide: "sideBySide", viewWipe: "wipe", viewHold: "hold" };
      Object.keys(modes).forEach((id) => {
        const button = $(id);
        button.setAttribute("aria-pressed", String(modes[id] === mode));
      });
      renderTransforms();
    }

    function sourceDimensions(image) {
      if (!image.naturalWidth || !image.naturalHeight) return null;
      return { width: image.naturalWidth, height: image.naturalHeight };
    }

    function imageTransform(image, viewport, pan) {
      const dims = sourceDimensions(image);
      if (!dims || viewport.clientWidth <= 0 || viewport.clientHeight <= 0) return;
      const scale = state.compareZoom === "100%"
        ? 1
        : QilViewer.fitScale(dims.width, dims.height, viewport.clientWidth, viewport.clientHeight);
      const renderedWidth = dims.width * scale;
      const renderedHeight = dims.height * scale;
      const overflowX = Math.max(0, renderedWidth - viewport.clientWidth);
      const overflowY = Math.max(0, renderedHeight - viewport.clientHeight);
      const x = (viewport.clientWidth - renderedWidth) / 2 - pan.x * overflowX;
      const y = (viewport.clientHeight - renderedHeight) / 2 - pan.y * overflowY;
      image.style.width = `${dims.width}px`;
      image.style.height = `${dims.height}px`;
      image.style.transform = `translate(${x}px, ${y}px) scale(${scale})`;
      return { renderedWidth, renderedHeight, overflowX, overflowY };
    }

    function renderTransforms() {
      const before = $("beforeShot");
      const after = $("afterShot");
      imageTransform(before, $("beforePane"), state.comparePan);
      imageTransform(after, $("afterPane"), state.comparePan);
      const wipe = $("wipeStage");
      imageTransform($("wipeBefore"), wipe, state.comparePan);
      imageTransform($("wipeAfter"), wipe, state.comparePan);
      const hold = $("holdStage");
      imageTransform($("holdBefore"), hold, state.comparePan);
      imageTransform($("holdAfter"), hold, state.comparePan);
      renderWipePosition();
    }

    function renderWipePosition() {
      const position = clamp(state.wipePosition, 100);
      state.wipePosition = position;
      $("wipeBeforeLayer").style.clipPath = `inset(0 ${100 - position}% 0 0)`;
      const divider = $("wipeDivider");
      divider.style.left = `${position}%`;
      divider.setAttribute("aria-valuenow", String(Math.round(position)));
    }

    function setZoom(zoom) {
      state.compareZoom = zoom;
      if (zoom === "fit") state.comparePan = { x: 0.5, y: 0.5 };
      $("zoomFit").setAttribute("aria-pressed", String(zoom === "fit"));
      $("zoom100").setAttribute("aria-pressed", String(zoom === "100%"));
      renderTransforms();
    }

    function updateDimensions() {
      const before = sourceDimensions($("beforeShot"));
      const after = sourceDimensions($("afterShot"));
      if (!before || !after) {
        $("compareNote").hidden = true;
        return;
      }
      $("beforeDims").textContent = `${before.width} × ${before.height}`;
      $("afterDims").textContent = `${after.width} × ${after.height}`;
      $("wipeDims").textContent = `${before.width} × ${before.height}　/　${after.width} × ${after.height}`;
      $("compareNote").hidden = before.width === after.width && before.height === after.height;
      renderTransforms();
    }

    function setImageFailure(id, failed) {
      $(id).hidden = !failed;
      if (id === "beforeError" || id === "afterError") {
        $("viewWipe").disabled = !$("beforeError").hidden || !$("afterError").hidden;
        $("viewHold").disabled = !$("beforeError").hidden || !$("afterError").hidden;
        if ($("viewHold").disabled && state.compareMode === "hold") setMode("result");
      }
    }

    function select(item) {
      const changed = selectedItem?.path !== item?.path;
      if (changed) releaseHold();
      selectedItem = item;
      const eligible = QilViewer.canCompare(item);
      $("viewer").classList.toggle("has-compare", eligible);
      $("viewerToolbar").hidden = !eligible;
      $("compareControls").hidden = !eligible;
      if (!eligible) {
        setMode("result");
        return;
      }
      $("beforeShot").alt = translate("before");
      $("afterShot").alt = translate("after");
      $("wipeBefore").alt = translate("before");
      $("wipeAfter").alt = translate("after");
      $("holdBefore").alt = translate("before");
      $("holdAfter").alt = translate("after");
      if (changed) {
        state.comparePan = { x: 0.5, y: 0.5 };
        state.compareZoom = "fit";
        state.wipePosition = 50;
        setMode("result");
        ["beforeError", "afterError"].forEach((id) => setImageFailure(id, false));
        const reference = fileSrc(item.meta.references[0]);
        const result = fileSrc(item.path);
        images.forEach((image) => { image.draggable = false; });
        $("beforeShot").onload = updateDimensions;
        $("afterShot").onload = updateDimensions;
        $("beforeShot").onerror = () => setImageFailure("beforeError", true);
        $("afterShot").onerror = () => setImageFailure("afterError", true);
        $("beforeShot").src = reference;
        $("wipeBefore").src = reference;
        $("holdBefore").src = reference;
        $("afterShot").src = result;
        $("wipeAfter").src = result;
        $("holdAfter").src = result;
        setZoom("fit");
      } else {
        setMode(state.compareMode);
      }
    }

    function reset() {
      releaseHold();
      selectedItem = null;
      setMode("result");
      $("viewer").classList.remove("has-compare");
      $("viewerToolbar").hidden = true;
      $("compareControls").hidden = true;
      $("zoomControls").hidden = true;
    }

    ["viewResult", "viewSideBySide", "viewWipe", "viewHold"].forEach((id) => {
      $(id).addEventListener("click", () => {
        const mode = { viewResult: "result", viewSideBySide: "sideBySide", viewWipe: "wipe", viewHold: "hold" }[id];
        if (mode === "wipe" && $("viewWipe").disabled) return;
        if (mode === "hold" && $("viewHold").disabled) return;
        setMode(mode);
        if (mode !== "result" && mode === "sideBySide") setZoom(state.compareZoom);
      });
    });
    $("holdStage").addEventListener("pointerdown", (event) => {
      if (state.compareMode !== "hold" || event.button !== 0 || holdingPointer !== null) return;
      event.preventDefault();
      beginHold(event.pointerId, $("holdStage"));
    });
    // The Hold button is also press-and-hold, so clicking it does something
    // immediately instead of only selecting the mode.
    $("viewHold").addEventListener("pointerdown", (event) => {
      if ($("viewHold").disabled || event.button !== 0 || holdingPointer !== null) return;
      if (state.compareMode !== "hold") setMode("hold");
      beginHold(event.pointerId, $("viewHold"));
    });
    function endHoldPointer(event) {
      if (holdingPointer !== null && event.pointerId === holdingPointer) releaseHold();
    }
    ["holdStage", "viewHold"].forEach((id) => {
      ["pointerup", "pointercancel", "lostpointercapture"].forEach((name) => $(id).addEventListener(name, endHoldPointer));
    });
    ["pointerup", "pointercancel"].forEach((name) => window.addEventListener(name, endHoldPointer));
    $("viewer").addEventListener("keydown", (event) => {
      const target = event.target;
      const isEditable = target?.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target?.tagName);
      const onHoldButton = target === $("viewHold");
      if (event.key !== " " || state.compareMode !== "hold" || isEditable || event.repeat) return;
      if (target !== $("viewer") && !onHoldButton) return;
      event.preventDefault();
      holdingSpace = true;
      renderHold();
    });
    $("viewer").addEventListener("keyup", (event) => {
      if (event.key === " " && holdingSpace) releaseHold();
    });
    window.addEventListener("blur", releaseHold);
    $("zoomFit").addEventListener("click", () => setZoom("fit"));
    $("zoom100").addEventListener("click", () => setZoom("100%"));
    $("compareView").addEventListener("wheel", (event) => {
      event.preventDefault();
      setZoom(event.deltaY < 0 ? "100%" : "fit");
    }, { passive: false });

    function panStart(event) {
      if (state.compareZoom !== "100%" || event.button !== 0) return;
      const pane = event.currentTarget.closest(".compare-pane, .wipe-stage");
      const image = event.currentTarget;
      const current = imageTransform(image, pane, state.comparePan);
      if (!current || (current.overflowX === 0 && current.overflowY === 0)) return;
      drag = { pointerId: event.pointerId, x: event.clientX, y: event.clientY, pan: { ...state.comparePan }, overflowX: current.overflowX, overflowY: current.overflowY };
      image.setPointerCapture(event.pointerId);
    }

    function panMove(event) {
      if (!drag || event.pointerId !== drag.pointerId) return;
      state.comparePan = {
        x: drag.overflowX ? clamp(drag.pan.x - (event.clientX - drag.x) / drag.overflowX, 1) : 0.5,
        y: drag.overflowY ? clamp(drag.pan.y - (event.clientY - drag.y) / drag.overflowY, 1) : 0.5,
      };
      renderTransforms();
    }

    function panEnd(event) {
      if (drag && event.pointerId === drag.pointerId) drag = null;
    }

    images.forEach((image) => {
      image.addEventListener("pointerdown", panStart);
      image.addEventListener("pointermove", panMove);
      image.addEventListener("pointerup", panEnd);
      image.addEventListener("pointercancel", panEnd);
    });

    function setWipeFromPointer(event) {
      const rect = $("wipeStage").getBoundingClientRect();
      state.wipePosition = clamp(((event.clientX - rect.left) / rect.width) * 100, 100);
      renderWipePosition();
    }
    $("wipeDivider").addEventListener("pointerdown", (event) => {
      event.currentTarget.setPointerCapture(event.pointerId);
      setWipeFromPointer(event);
    });
    $("wipeDivider").addEventListener("pointermove", (event) => {
      if (event.currentTarget.hasPointerCapture(event.pointerId)) setWipeFromPointer(event);
    });
    $("wipeDivider").addEventListener("pointerup", (event) => event.currentTarget.releasePointerCapture(event.pointerId));
    $("wipeDivider").addEventListener("keydown", (event) => {
      const step = event.shiftKey ? 10 : 1;
      if (event.key === "ArrowLeft") state.wipePosition -= step;
      else if (event.key === "ArrowRight") state.wipePosition += step;
      else return;
      event.preventDefault();
      renderWipePosition();
    });

    const resize = new ResizeObserver(renderTransforms);
    resize.observe($("beforePane"));
    resize.observe($("afterPane"));
    resize.observe($("wipeStage"));
    window.addEventListener("langchange", () => {
      if (selectedItem) select(selectedItem);
    });

    return Object.freeze({ select, reset, renderTransforms });
  }

  window.QilComparison = Object.freeze({ create });
})();
