(() => {
  const clamp = (value, max) => Math.min(Math.max(value, 0), max);

  function canCompare(item) {
    const refs = item?.meta?.references;
    return item?.meta?.mode === "photos" && Array.isArray(refs) && typeof refs[0] === "string" && refs[0].length > 0;
  }

  function fitScale(sourceWidth, sourceHeight, viewportWidth, viewportHeight) {
    if ([sourceWidth, sourceHeight, viewportWidth, viewportHeight].some((size) => !Number.isFinite(size) || size <= 0)) return 0;
    return Math.min(viewportWidth / sourceWidth, viewportHeight / sourceHeight);
  }

  function normalizedPan(point, sourceSize, viewportSize) {
    const axis = (offset, source, viewport) => {
      const overflow = Math.max(0, source - viewport);
      return overflow === 0 ? 0.5 : clamp(offset, overflow) / overflow;
    };
    return {
      x: axis(point.x, sourceSize.width, viewportSize.width),
      y: axis(point.y, sourceSize.height, viewportSize.height),
    };
  }

  const api = Object.freeze({ canCompare, fitScale, normalizedPan });
  if (typeof module === "object" && module.exports) module.exports = api;
  else window.QilViewer = api;
})();
