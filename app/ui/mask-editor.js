(() => {
  const MAX_HISTORY = 100;
  const MIN_BRUSH = 1;
  const MAX_BRUSH = 512;

  // Maps CSS-pixel viewport coordinates to source-image pixels. The image is
  // fitted and letterboxed in the viewport, then zoomed about its centre and
  // panned (pan is in CSS pixels). Device pixel ratio is deliberately absent:
  // pointer events are in CSS pixels, so DPR never changes mask coordinates.
  function viewTransform({ width, height, viewWidth, viewHeight, zoom = 1, panX = 0, panY = 0 }) {
    const fit = Math.min(viewWidth / width, viewHeight / height);
    const scale = fit * zoom;
    const offsetX = (viewWidth - width * scale) / 2 + panX;
    const offsetY = (viewHeight - height * scale) / 2 + panY;
    return { scale, offsetX, offsetY };
  }

  function toSource(t, cssX, cssY) {
    return { x: (cssX - t.offsetX) / t.scale, y: (cssY - t.offsetY) / t.scale };
  }

  function toView(t, x, y) {
    return { x: x * t.scale + t.offsetX, y: y * t.scale + t.offsetY };
  }

  function canvasBackingSize(viewWidth, viewHeight, dpr) {
    return { width: Math.round(viewWidth * dpr), height: Math.round(viewHeight * dpr) };
  }

  function clamp(v, lo, hi) {
    return Math.min(hi, Math.max(lo, v));
  }

  // Paints one round-capped stroke (a chain of stamped discs) into a w*h buffer.
  function paintStroke(buf, width, height, stroke) {
    const value = stroke.tool === "eraser" ? 0 : 255;
    const r = stroke.radius;
    const pts = stroke.points;
    const stamp = (cx, cy) => {
      const x0 = Math.max(0, Math.floor(cx - r));
      const x1 = Math.min(width - 1, Math.ceil(cx + r));
      const y0 = Math.max(0, Math.floor(cy - r));
      const y1 = Math.min(height - 1, Math.ceil(cy + r));
      for (let y = y0; y <= y1; y++) {
        for (let x = x0; x <= x1; x++) {
          const dx = x + 0.5 - cx;
          const dy = y + 0.5 - cy;
          if (dx * dx + dy * dy <= r * r) buf[y * width + x] = value;
        }
      }
    };
    for (let i = 0; i < pts.length; i++) {
      const p = pts[i];
      stamp(p.x, p.y);
      if (i > 0) {
        const q = pts[i - 1];
        const dist = Math.hypot(p.x - q.x, p.y - q.y);
        const steps = Math.ceil(dist / Math.max(1, r / 2));
        for (let s = 1; s < steps; s++) {
          const f = s / steps;
          stamp(q.x + (p.x - q.x) * f, q.y + (p.y - q.y) * f);
        }
      }
    }
  }

  // deps: { encodePng(gray, width, height) -> Promise<Uint8Array>, onChange() }
  function create(deps = {}) {
    let size = null; // { width, height }
    let source = null;
    let strokes = [];
    let baseStrokes = []; // strokes aged out of undo history, still painted
    let redoStack = [];
    let current = null;
    let tool = "brush";
    let brush = 32;
    let busy = false;
    let cache = null;

    const changed = () => {
      cache = null;
      if (deps.onChange) deps.onChange();
    };
    const requireOpen = () => {
      if (!size) throw new Error("mask editor is not open");
    };
    const reset = () => {
      strokes = [];
      baseStrokes = [];
      redoStack = [];
      current = null;
    };

    function open({ source: src, width, height }) {
      if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 1) {
        throw new Error("invalid source dimensions");
      }
      source = src;
      size = { width, height };
      reset();
      changed();
    }

    function close() {
      size = null;
      source = null;
      reset();
      changed();
    }

    function addPoint(x, y) {
      if (!current) return;
      current.points.push({ x: clamp(x, 0, size.width), y: clamp(y, 0, size.height) });
      changed();
    }

    function beginStroke(x, y) {
      requireOpen();
      if (busy) return false;
      current = { tool, radius: brush / 2, points: [] };
      addPoint(x, y);
      return true;
    }

    function endStroke() {
      if (!current) return;
      strokes.push(current);
      current = null;
      if (strokes.length > MAX_HISTORY) baseStrokes.push(strokes.shift());
      redoStack = [];
      changed();
    }

    function undo() {
      if (busy || current || strokes.length === 0) return false;
      redoStack.push(strokes.pop());
      changed();
      return true;
    }

    function redo() {
      if (busy || current || redoStack.length === 0) return false;
      strokes.push(redoStack.pop());
      changed();
      return true;
    }

    function clear() {
      if (busy) return false;
      reset();
      changed();
      return true;
    }

    function setBrushSize(px) {
      if (!Number.isFinite(px)) return;
      brush = clamp(Math.round(px), MIN_BRUSH, MAX_BRUSH);
    }

    function setTool(next) {
      if (next !== "brush" && next !== "eraser") throw new Error("unknown tool");
      tool = next;
    }

    function setBusy(value) {
      busy = Boolean(value);
      if (busy && current) {
        current = null;
        changed();
      }
    }

    // Grayscale mask, row-major, exactly width*height; 255 = editable.
    function rasterize() {
      requireOpen();
      if (cache) return cache;
      const buf = new Uint8Array(size.width * size.height);
      for (const s of baseStrokes) paintStroke(buf, size.width, size.height, s);
      for (const s of strokes) paintStroke(buf, size.width, size.height, s);
      if (current) paintStroke(buf, size.width, size.height, current);
      cache = buf;
      return buf;
    }

    function isEmpty() {
      return !rasterize().some((v) => v !== 0);
    }

    // True once anything has been drawn since open/clear, even if erased again,
    // so a source switch always asks before discarding user work.
    function isDirty() {
      return size !== null && (strokes.length > 0 || baseStrokes.length > 0 || current !== null);
    }

    async function getMaskPng() {
      requireOpen();
      if (busy) throw new Error("mask editor is busy");
      if (isEmpty()) throw new Error("mask is empty");
      if (!deps.encodePng) throw new Error("no PNG encoder configured");
      return deps.encodePng(rasterize(), size.width, size.height);
    }

    return {
      open,
      close,
      clear,
      undo,
      redo,
      beginStroke,
      addPoint,
      endStroke,
      setBrushSize,
      setTool,
      setBusy,
      rasterize,
      isEmpty,
      isDirty,
      getMaskPng,
      get size() { return size && { ...size }; },
      get source() { return source; },
      get tool() { return tool; },
      get brushSize() { return brush; },
      get busy() { return busy; },
    };
  }

  const api = Object.freeze({ create, viewTransform, toSource, toView, canvasBackingSize });
  if (typeof module === "object" && module.exports) module.exports = api;
  else window.QilMaskEditor = api;
})();
