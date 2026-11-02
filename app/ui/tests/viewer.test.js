const assert = require("node:assert/strict");
const test = require("node:test");
const { canCompare, fitScale, normalizedPan } = require("../viewer.js");

test("photo and masked edits require a valid first reference for comparison", () => {
  assert.equal(canCompare({ meta: { mode: "photos", references: ["original.png"] } }), true);
  assert.equal(canCompare({ meta: { mode: "text", references: ["original.png"] } }), false);
  assert.equal(canCompare({ meta: { mode: "photos", references: [] } }), false);
  assert.equal(canCompare({ meta: { mode: "erase", references: ["source.png"] } }), true);
  assert.equal(canCompare({ meta: { mode: "erase", references: [] } }), false);
  assert.equal(canCompare({ meta: { mode: "masked-edit", references: [] } }), false);
  assert.equal(canCompare({ meta: { mode: "masked-edit", references: [null] } }), false);
  assert.equal(canCompare({ meta: { mode: "masked-edit", references: [""] } }), false);
  assert.equal(canCompare({ meta: null }), false);
});

test("contain fit keeps the complete image inside its viewport", () => {
  assert.equal(fitScale(2000, 1000, 1000, 800), 0.5);
  assert.equal(fitScale(500, 1000, 1000, 800), 0.8);
  assert.equal(fitScale(0, 1000, 1000, 800), 0);
});

test("pan normalizes and clamps each image against its own overflow", () => {
  assert.deepEqual(
    normalizedPan({ x: 250, y: 500 }, { width: 1000, height: 1000 }, { width: 500, height: 500 }),
    { x: 0.5, y: 1 },
  );
  assert.deepEqual(
    normalizedPan({ x: -5, y: 10 }, { width: 400, height: 600 }, { width: 500, height: 500 }),
    { x: 0.5, y: 0.1 },
  );
});
