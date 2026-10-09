const assert = require("node:assert/strict");
const test = require("node:test");
const { createBatch, remainingMs, requestSeed, shouldStartNext } = require("../generation-batch.js");

test("finite batch counts and deadline include bounded handoff time", () => {
  const batch = createBatch(4, 10_000, false, 123);

  assert.equal(batch.count, 4);
  assert.equal(batch.deadlineMs, 10_000 + 4 * 30 * 60 * 1000 + 3 * 2 * 60 * 1000);
  assert.equal(batch.seed, null);
});

test("locked seed is preserved for a repeated batch", () => {
  const batch = createBatch(8, 0, true, 12345);

  assert.equal(batch.seedLocked, true);
  assert.equal(batch.seed, 12345);
  assert.equal(requestSeed(batch), 12345);
});

test("unlocked batch leaves the seed random for each request", () => {
  assert.equal(requestSeed(createBatch(4, 0, false, 12345)), null);
});

test("finite batch stops at its count or requested stop", () => {
  const batch = createBatch(4, 0, false, null);

  assert.equal(batch.requestActive, false);
  assert.equal(shouldStartNext(batch, 1), true);
  batch.requestActive = true;
  batch.completed = 4;
  assert.equal(shouldStartNext(batch, 1), false);
  batch.completed = 1;
  batch.stopAfterCurrent = true;
  assert.equal(shouldStartNext(batch, 1), false);
});

test("finite batch stops when its total deadline expires", () => {
  const batch = createBatch(4, 0, false, null);

  assert.equal(shouldStartNext(batch, batch.deadlineMs), false);
  assert.equal(remainingMs(batch, batch.deadlineMs + 1), 0);
});

test("forever batch has no total deadline and continues until stopped", () => {
  const batch = createBatch(null, 0, false, null);

  assert.equal(batch.deadlineMs, null);
  assert.equal(remainingMs(batch, 1_000_000), null);
  assert.equal(shouldStartNext(batch, 1_000_000), true);
  batch.stopAfterCurrent = true;
  assert.equal(shouldStartNext(batch, 1_000_000), false);
});
