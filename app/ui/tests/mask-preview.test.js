import assert from "node:assert/strict";
import { createRequire } from "node:module";
import test from "node:test";
const require = createRequire(import.meta.url);
const { create } = require("../mask-preview.js");
import { readFileSync } from "node:fs";
const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const maskUi = readFileSync(new URL("../mask-editor-ui.js", import.meta.url), "utf8");

test("preview requests default to radius 8 and inward mode, then send mode changes", async () => {
  const calls = [];
  const preview = create({ invoke: async (payload) => { calls.push(payload); return "png"; } });
  await preview.request({ maskPngBase64: "raw", revision: 1 });
  await preview.request({ maskPngBase64: "raw2", featherPx: 20, featherMode: "bothSides", revision: 2 });
  assert.deepEqual(calls.map((call) => [call.featherPx, call.featherMode]), [[8, "inward"], [20, "bothSides"]]);
});

test("preview only commits the newest revision and ignores invalidated sources", async () => {
  const pending = [];
  const shown = [];
  const preview = create({ invoke: (payload) => new Promise((resolve) => pending.push({ payload, resolve })), onResult: (r) => shown.push(r) });
  const older = preview.request({ maskPngBase64: "old", revision: 1, source: "a" });
  const newer = preview.request({ maskPngBase64: "new", revision: 2, source: "a" });
  pending[0].resolve("old png");
  await Promise.resolve();
  pending[1].resolve("new png");
  await Promise.all([older, newer]);
  assert.deepEqual(shown, ["new png"]);
  const stale = preview.request({ maskPngBase64: "stale", revision: 3, source: "a" });
  preview.invalidate("b");
  pending[2].resolve("stale png");
  await stale;
  assert.deepEqual(shown, ["new png"]);
});

test("only one IPC preview is active and at most one newest request waits", async () => {
  const calls = [];
  const pending = [];
  const preview = create({ invoke: (payload) => { calls.push(payload.maskPngBase64); return new Promise((resolve) => pending.push(resolve)); } });
  const first = preview.request({ maskPngBase64: "1", revision: 1 });
  const second = preview.request({ maskPngBase64: "2", revision: 2 });
  const third = preview.request({ maskPngBase64: "3", revision: 3 });
  assert.deepEqual(calls, ["1"]);
  pending.shift()("one");
  await Promise.resolve();
  assert.deepEqual(calls, ["1", "3"]);
  pending.shift()("three");
  await Promise.all([first, second, third]);
});

test("mask feather controls default to inward radius 8 and warn about Gaussian spill", () => {
  assert.match(html, /id="maskFeather"[^>]*min="0"[^>]*max="64"[^>]*step="1"[^>]*value="8"/);
  assert.match(html, /id="maskFeatherMode"[\s\S]*?option value="inward"[\s\S]*?option value="bothSides"/);
  assert.match(maskUi, /maskGaussianWarning/);
  assert.match(maskUi, /function onPointerDown[\s\S]*?schedulePreview\(\)/);
  assert.match(maskUi, /function onPointerMove[\s\S]*?schedulePreview\(\)/);
  assert.match(maskUi, /function syncOverlay\([\s\S]*?schedulePreview\(\)/);
});
