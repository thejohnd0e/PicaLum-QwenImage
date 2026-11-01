import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

test("generation request carries cfg and negative", () => {
  const src = readFileSync(new URL("../app.js", import.meta.url), "utf8");
  assert.match(src, /cfg:\s*snapshot\.cfg/);
  assert.match(src, /negative:\s*snapshot\.negative/);
  assert.match(src, /cfg:\s*Number\(\$\("cfg"\)\.value\)/);
  assert.match(src, /negative:\s*\$\("negative"\)\.value\.trim\(\)/);
});

test("defaults preserve historic prompt-adherence settings", () => {
  const src = readFileSync(new URL("../app.js", import.meta.url), "utf8");
  assert.match(src, /cfg:\s*Number\(localGet\("cfg",\s*"1"\)\)/);
  assert.match(src, /negative:\s*localGet\("negative",\s*""\)/);
});
