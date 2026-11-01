import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const app = readFileSync(new URL("../app.js", import.meta.url), "utf8");
const maskUI = readFileSync(new URL("../mask-editor-ui.js", import.meta.url), "utf8");

test("photo edit denoising controls have 0.10–1.00 range, hundredth steps, and 0.75 default", () => {
  assert.match(html, /id="denoisingStrength"[^>]*type="range"[^>]*min="0\.10"[^>]*max="1\.00"[^>]*step="0\.01"[^>]*value="0\.75"/);
  assert.match(html, /id="denoisingStrengthNumber"[^>]*type="number"[^>]*min="0\.10"[^>]*max="1\.00"[^>]*step="0\.01"[^>]*value="0\.75"/);
});

test("main generation snapshots CFG and denoising strength into each request", () => {
  assert.match(app, /denoisingStrength:\s*Number\(\$\("denoisingStrength"\)\.value\)/);
  assert.match(app, /denoisingStrength:\s*snapshot\.mode === "photos" \? snapshot\.denoisingStrength : 1\.0/);
  assert.match(app, /cfg:\s*snapshot\.cfg/);
});

test("denoising UI is photo-only and mask dialog synchronizes hundredth precision", () => {
  assert.match(html, /data-show="photos"[^>]*id="photoDenoisingControls"|id="photoDenoisingControls"[^>]*data-show="photos"/);
  assert.match(maskUI, /maskDenoisingStrengthNumber/);
  assert.match(maskUI, /Number\(\$\("maskDenoisingStrength"\)\.value\)/);
});
