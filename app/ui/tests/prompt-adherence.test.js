import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

test("generation request carries cfg and negative", () => {
  const src = readFileSync(new URL("../app.js", import.meta.url), "utf8");
  assert.match(src, /cfg:\s*snapshot\.cfg/);
  assert.match(src, /negative:\s*snapshot\.negative/);
  assert.match(src, /cfg:\s*state\.cfg/);
  assert.match(src, /negative:\s*\$\("negative"\)\.value\.trim\(\)/);
});

test("defaults preserve historic prompt-adherence settings", () => {
  const src = readFileSync(new URL("../app.js", import.meta.url), "utf8");
  assert.match(src, /cfg:\s*Number\(localGet\("cfg",\s*"1"\)\)/);
  assert.match(src, /negative:\s*localGet\("negative",\s*""\)/);
});

test("CFG controls use the agreed range, precision, and a matching numeric input", () => {
  const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
  assert.match(html, /id="cfg"[^>]*type="range"[^>]*min="1"[^>]*max="5"[^>]*step="0\.1"/);
  assert.match(html, /id="cfgNumber"[^>]*type="number"[^>]*min="1"[^>]*max="5"[^>]*step="0\.1"/);
});
