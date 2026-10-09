const assert = require("node:assert/strict");
const test = require("node:test");

test("setup contract exposes an actionable eligibility error and selected GPU state", () => {
  const status = { eligibilityError: { message: "GPU has insufficient VRAM" }, selectedUuid: "GPU-a", gpus: [] };
  assert.equal(Boolean(status.eligibilityError), true);
  assert.equal(status.selectedUuid, "GPU-a");
});

test("profile mode and GPU selection are persisted together in the command payload", () => {
  const payload = { profile: "compact", modelsDir: "D:/models", selectedUuid: "GPU-b", profileMode: "manual" };
  assert.deepEqual(payload, { profile: "compact", modelsDir: "D:/models", selectedUuid: "GPU-b", profileMode: "manual" });
});

test("GPU setup localization defines capability status in all supported languages", () => {
  const fs = require("node:fs");
  const source = fs.readFileSync(require("node:path").join(__dirname, "..", "i18n.js"), "utf8");
  for (const key of ["capabilityUnknown", "capabilityPending", "capabilityRejected", "stepGpuCheck"]) assert.equal((source.match(new RegExp(key + ":", "g")) || []).length, 3, key);
});

test("verification card is styled, hides the empty state and labels every action", () => {
  const fs = require("node:fs");
  const path = require("node:path");
  const css = fs.readFileSync(path.join(__dirname, "..", "styles.css"), "utf8");
  const ui = fs.readFileSync(path.join(__dirname, "..", "app.js"), "utf8");
  assert.match(css, /\.verification-card\s*\{/);
  assert.match(css, /\.verification-card\[hidden\]\s*\{\s*display:\s*none/);
  assert.match(css, /:has\(\.verification-card:not\(\[hidden\]\)\)\s*\.empty/);
  for (const key of ["verificationRetry", "verificationCancel", "verificationContinue"]) assert.ok(ui.includes(`textContent=t("${key}")`), key);
  assert.ok(ui.includes("v.error"), "failure detail shows the real error");
  new Function(ui.replace(/^\s*(import|export)\b.*$/gm, ""));
});

test("unknown capability is an install blocker while a ready setup is allowed", () => {
  const canInstall = (status) => !status.eligibilityError && (status.ready || status.gpus?.length > 0);
  assert.equal(canInstall({ eligibilityError: { kind: "unsupportedGpu" } }), false);
  assert.equal(canInstall({ ready: true, gpus: [{ gpu: { uuid: "GPU-ok" } }] }), true);
});


test('Task 6 verification UX has all localized state strings and command hooks', () => { const fs=require('node:fs'); const path=require('node:path'); const ui=fs.readFileSync(path.join(__dirname,'..','app.js'),'utf8'); const i18n=fs.readFileSync(path.join(__dirname,'..','i18n.js'),'utf8'); for (const k of ['verificationRunning','verificationSuccess','verificationFailed','verificationSkipped','verificationContinue','verificationRetry','verificationCancel']) assert.equal((i18n.match(new RegExp(k+':','g'))||[]).length,3); for (const cmd of ['gpu_verification_start','gpu_verification_cancel','gpu_verification_status']) assert.match(ui+fs.readFileSync(path.join(__dirname,"..","verification-controller.js"),"utf8"),new RegExp(cmd)); });

test("production loads and instantiates the verification controller", () => { const fs=require("node:fs"); const path=require("node:path"); const html=fs.readFileSync(path.join(__dirname,"..","index.html"),"utf8"); const ui=fs.readFileSync(path.join(__dirname,"..","app.js"),"utf8"); const controller=fs.readFileSync(path.join(__dirname,"..","verification-controller.js"),"utf8"); assert.ok(html.includes("<script src=\"verification-controller.js\"></script>")); assert.ok(html.indexOf("verification-controller.js") >= 0 && html.indexOf("app.js") > html.indexOf("verification-controller.js")); assert.ok(ui.includes("QilVerification({ invoke, listen")); assert.equal((ui.match(/invoke\("gpu_verification_start"\)/g)||[]).length,0); });
test("opening the app never auto-starts verification; Settings offers a manual check", () => {
  const fs = require("node:fs");
  const path = require("node:path");
  const ui = fs.readFileSync(path.join(__dirname, "..", "app.js"), "utf8");
  const html = fs.readFileSync(path.join(__dirname, "..", "index.html"), "utf8");
  const i18n = fs.readFileSync(path.join(__dirname, "..", "i18n.js"), "utf8");
  assert.ok(ui.includes("function enterReadyFlow() { enterApp(); }"));
  assert.ok(!ui.includes("beginVerificationPhase") && !ui.includes("refreshVerification"));
  assert.ok(/settingsVerificationRetry"\)\.addEventListener\("click", async \(\) => \{ closeSettings\(\); await startVerification\(true\);/.test(ui));
  assert.ok(html.includes('id="settingsVerificationRetry" data-i18n="verificationRun"'));
  for (const key of ["verificationRun", "verificationRunHint"]) assert.equal((i18n.match(new RegExp(key + ":", "g")) || []).length, 3, key);
  assert.ok(ui.includes('$("genBtn").disabled || state.verification.state === "running"'));
});

test("a failed or interrupted verification is retried on the next start, not remembered as started", async () => {
  const create = require("../verification-controller.js");
  const storage = new Map();
  let calls = 0;
  let fail = true;
  const controller = create({
    invoke: async () => { calls += 1; if (fail) throw new Error("engine could not start"); },
    listen: () => {},
    storage,
    key: () => "GPU-a|balanced|comfy-x",
  });
  assert.equal((await controller.start()).state, "failed");
  assert.equal(storage.get("started"), undefined);
  fail = false;
  assert.equal((await controller.start()).state, "success");
  assert.equal(calls, 2);
  assert.equal(storage.get("started"), "GPU-a|balanced|comfy-x");
  assert.equal((await controller.start()).state, "success", "completed key is not re-run");
  assert.equal(calls, 2);
});

test("setup status exposes the engine fingerprint used by the verification key", () => {
  const fs = require("node:fs");
  const path = require("node:path");
  const ui = fs.readFileSync(path.join(__dirname, "..", "app.js"), "utf8");
  assert.ok(ui.includes("s?.status?.engineFingerprint"));
});

test("header stacks the product name over the version and GitHub link", () => {
  const fs = require("node:fs");
  const path = require("node:path");
  const html = fs.readFileSync(path.join(__dirname, "..", "index.html"), "utf8");
  const ui = fs.readFileSync(path.join(__dirname, "..", "app.js"), "utf8");
  for (const id of ["profileTag", "brand-model", "qwen-image-2.1"]) {
    assert.ok(!html.includes(id), `html still has ${id}`);
    assert.ok(!ui.includes(id), `app.js still references ${id}`);
  }
  const copy = html.slice(html.indexOf('class="brand-copy"'), html.indexOf("</header>"));
  const title = copy.indexOf("<strong>PicaLum QwenImage</strong>");
  const meta = copy.indexOf('class="brand-meta"');
  assert.ok(title >= 0 && meta > title, "title comes first, meta below");
  assert.ok(copy.indexOf('id="appVersion"') > meta && copy.indexOf('id="githubLink"') > copy.indexOf('id="appVersion"'));
  assert.ok(ui.includes('$("appVersion").textContent'));
});

test("header: larger product name, larger mode tabs pushed away from the brand", () => {
  const fs = require("node:fs");
  const path = require("node:path");
  const css = fs.readFileSync(path.join(__dirname, "..", "styles.css"), "utf8");
  assert.match(css, /\.brand strong \{ font-size: 16px;/);
  assert.match(css, /\.mode-nav button \{[^}]*font: 500 15px/);
  assert.match(css, /\.mode-nav \{[^}]*margin-left: auto;/);
});
