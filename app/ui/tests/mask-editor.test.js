const assert = require("node:assert/strict");
const test = require("node:test");
const { create, viewTransform, toSource, toView, canvasBackingSize } = require("../mask-editor.js");

const px = (ed, x, y) => ed.rasterize()[y * ed.size.width + x];
const opened = (w = 40, h = 20, deps) => {
  const ed = create(deps);
  ed.open({ source: "a.png", width: w, height: h });
  return ed;
};
const stroke = (ed, pts) => {
  ed.beginStroke(pts[0][0], pts[0][1]);
  for (const p of pts.slice(1)) ed.addPoint(p[0], p[1]);
  ed.endStroke();
};

test("fit letterboxes a wide image in a square view", () => {
  const t = viewTransform({ width: 200, height: 100, viewWidth: 100, viewHeight: 100 });
  assert.equal(t.scale, 0.5);
  assert.equal(t.offsetX, 0);
  assert.equal(t.offsetY, 25);
  assert.deepEqual(toSource(t, 50, 50), { x: 100, y: 50 });
  assert.deepEqual(toView(t, 100, 50), { x: 50, y: 50 });
});

test("zoom and pan are applied exactly and round-trip", () => {
  const t = viewTransform({ width: 100, height: 100, viewWidth: 200, viewHeight: 100, zoom: 2, panX: 10, panY: -5 });
  assert.equal(t.scale, 2);
  const s = toSource(t, 123, 77);
  const v = toView(t, s.x, s.y);
  assert.ok(Math.abs(v.x - 123) < 1e-9 && Math.abs(v.y - 77) < 1e-9);
  // image centre sits at view centre plus pan
  assert.deepEqual(toSource(t, 100 + 10, 50 - 5), { x: 50, y: 50 });
});

test("DPR changes backing size only, never logical mask size", () => {
  assert.deepEqual(canvasBackingSize(100, 50, 2), { width: 200, height: 100 });
  assert.deepEqual(canvasBackingSize(100, 50, 1.5), { width: 150, height: 75 });
  assert.deepEqual(opened(40, 20).size, { width: 40, height: 20 });
});

test("brush paints white, eraser restores black", () => {
  const ed = opened();
  ed.setBrushSize(8);
  stroke(ed, [[20, 10]]);
  assert.equal(px(ed, 20, 10), 255);
  assert.equal(px(ed, 0, 0), 0);
  ed.setTool("eraser");
  stroke(ed, [[20, 10]]);
  assert.equal(px(ed, 20, 10), 0);
  assert.equal(ed.isEmpty(), true);
});

test("strokes connect and clamp to source bounds", () => {
  const ed = opened();
  ed.setBrushSize(4);
  stroke(ed, [[-50, 10], [100, 10]]);
  assert.equal(px(ed, 0, 10), 255);
  assert.equal(px(ed, 39, 10), 255);
  assert.equal(px(ed, 20, 10), 255);
  assert.equal(ed.rasterize().length, 40 * 20);
});

test("undo and redo restore mask state", () => {
  const ed = opened();
  ed.setBrushSize(6);
  stroke(ed, [[10, 10]]);
  stroke(ed, [[30, 10]]);
  assert.equal(px(ed, 30, 10), 255);
  assert.ok(ed.undo());
  assert.equal(px(ed, 30, 10), 0);
  assert.equal(px(ed, 10, 10), 255);
  assert.ok(ed.redo());
  assert.equal(px(ed, 30, 10), 255);
  ed.undo();
  stroke(ed, [[5, 5]]);
  assert.equal(ed.redo(), false, "new stroke clears redo");
});

test("clear and re-open reset the mask and dirty state", () => {
  const ed = opened();
  assert.equal(ed.isDirty(), false);
  stroke(ed, [[10, 10]]);
  assert.equal(ed.isDirty(), true);
  ed.clear();
  assert.equal(ed.isEmpty(), true);
  assert.equal(ed.isDirty(), false);
  stroke(ed, [[10, 10]]);
  ed.open({ source: "b.png", width: 10, height: 10 });
  assert.equal(ed.isEmpty(), true);
  assert.deepEqual(ed.size, { width: 10, height: 10 });
});

test("history is bounded without changing the mask", () => {
  const ed = opened(200, 20);
  ed.setBrushSize(2);
  for (let i = 0; i < 130; i++) stroke(ed, [[i + 1, 10]]);
  let undone = 0;
  while (ed.undo()) undone++;
  assert.equal(undone, 100);
  assert.equal(px(ed, 1, 10), 255, "oldest strokes stay baked in");
  assert.equal(px(ed, 129, 10), 0);
});

test("serialization uses source size and white-on-black polarity", async () => {
  let seen;
  const ed = opened(40, 20, {
    encodePng: async (gray, w, h) => {
      seen = { gray, w, h };
      return new Uint8Array([1]);
    },
  });
  await assert.rejects(ed.getMaskPng(), /empty/);
  ed.setBrushSize(10);
  stroke(ed, [[20, 10]]);
  await ed.getMaskPng();
  assert.equal(seen.w, 40);
  assert.equal(seen.h, 20);
  assert.equal(seen.gray.length, 800);
  assert.equal(seen.gray[10 * 40 + 20], 255);
  assert.equal(seen.gray[0], 0);
});

test("busy state blocks editing and submission", async () => {
  const ed = opened(40, 20, { encodePng: async () => new Uint8Array() });
  stroke(ed, [[10, 10]]);
  ed.setBusy(true);
  assert.equal(ed.beginStroke(5, 5), false);
  assert.equal(ed.undo(), false);
  assert.equal(ed.clear(), false);
  await assert.rejects(ed.getMaskPng(), /busy/);
  ed.setBusy(false);
  assert.equal(ed.undo(), true);
});

test("brush size is clamped and tool validated", () => {
  const ed = opened();
  ed.setBrushSize(0);
  assert.equal(ed.brushSize, 1);
  ed.setBrushSize(10000);
  assert.equal(ed.brushSize, 512);
  assert.throws(() => ed.setTool("lasso"));
});
