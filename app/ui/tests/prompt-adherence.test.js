import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

test("desktop generation retains CFG but omits negative-prompt state and payload", () => {
  const src = readFileSync(new URL("../app.js", import.meta.url), "utf8");
  const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
  assert.match(src, /cfg:\s*snapshot\.cfg/);
  assert.match(src, /cfg:\s*state\.cfg/);
  assert.doesNotMatch(src, /localGet\("negative"/);
  assert.doesNotMatch(src, /\$\("negative"\)/);
  assert.doesNotMatch(src, /negative:\s*snapshot\.negative/);
  assert.doesNotMatch(html, /id="negative"/);
});

test("legacy saved negative text is not loaded into desktop state", () => {
  const src = readFileSync(new URL("../app.js", import.meta.url), "utf8");
  assert.doesNotMatch(src, /negative:\s*localGet\("negative"/);
  assert.doesNotMatch(src, /localSet\("negative"/);
});

test("CFG controls use the agreed range, precision, and a matching numeric input", () => {
  const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
  assert.match(html, /id="cfg"[^>]*type="range"[^>]*min="1"[^>]*max="5"[^>]*step="0\.1"/);
  assert.match(html, /id="cfgNumber"[^>]*type="number"[^>]*min="1"[^>]*max="5"[^>]*step="0\.1"/);
});
