import assert from "node:assert/strict";
import test from "node:test";
import qualityControls from "../quality-controls.js";
import { readFileSync } from "node:fs";

function element({ value = "", min = "", max = "", step = "", open = false, dataset = {} } = {}) {
  return {
    value, min, max, step, open, disabled: false, hidden: false, textContent: "",
    attributes: {}, listeners: {}, validityMessage: "", dataset,
    addEventListener(type, fn) { this.listeners[type] = fn; },
    setAttribute(name, value) { this.attributes[name] = String(value); },
    setCustomValidity(message) { this.validityMessage = message; },
    checkValidity() {
      if (this.validityMessage || this.value === "") return false;
      const value = Number(this.value);
      return Number.isFinite(value) && (!this.min || value >= Number(this.min))
        && (!this.max || value <= Number(this.max))
        && (!this.step || Number.isInteger(value / Number(this.step)));
    },
    reportValidity() { this.reported = true; },
  };
}

function setup({ saved = {}, invoke, onChange = () => {} } = {}) {
  const nodes = {
    qualityPresets: element(), qualitySummary: element(), qualityPresetMeta: element(),
    qualitySteps: element({ value: "30", min: "1", max: "100", step: "1" }),
    qualityWidth: element({ value: "1280", min: "256", max: "4096", step: "32" }),
    qualityHeight: element({ value: "1280", min: "256", max: "4096", step: "32" }),
    qualitySizePair: element(), qualitySizeAuto: element({ hidden: true }),
    qualityStatus: element(), qualityError: element(), qualityWarning: element(),
    qualityPresetDraft: element({ dataset: { t: "1024² · 20 · ~35 s", e: "1024 px · 20 · ~50 s" } }),
    qualityPresetStandard: element({ dataset: { t: "1.6 MP · 30 · ~1.5 min", e: "1280 px · 30 · ~2.5 min" } }),
    qualityPresetMax: element({ dataset: { t: "2K · 40 · ~7 min", e: "1536 px · 40 · ~5 min" } }),
  };
  const values = new Map(Object.entries(saved));
  const calls = [];
  const api = qualityControls.create({
    getElement: (id) => nodes[id],
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (invoke) return invoke(command, args);
      const request = args.request;
      const presetSettings = {
        draft: { steps: 20, width: 1024, height: 1024 },
        standard: { steps: 30, width: 1280, height: 1280 },
        max: { steps: 40, width: 2048, height: 2048 },
      }[request.preset];
      return { ...presetSettings, ...(request.width ? { width: request.width, height: request.height } : {}) };
    },
    translate: (key, ...args) => args.length ? `${key}:${args.join(",")}` : key,
    storage: {
      get: (key) => values.get(key) ?? null,
      set: (key, value) => values.set(key, value),
    },
    onChange,
  });
  return { api, nodes, values, calls };
}

test("quality UI is one collapsed card with segmented presets and bounded manual controls", () => {
  const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
  assert.match(html, /<details[^>]*id="qualityPresets"/);
  assert.match(html, /<summary[^>]*>[\s\S]*data-i18n="qualityPresets"[\s\S]*id="qualitySummary"/);
  assert.match(html, /class="seg"[^>]*id="presets"[^>]*role="radiogroup"/);
  assert.equal((html.match(/data-preset="(draft|standard|max)"/g) || []).length, 3);
  const quality = html.slice(html.indexOf('id="qualityPresets"'), html.indexOf("</details>", html.indexOf('id="qualityPresets"')));
  assert.doesNotMatch(quality, /p-meta/);
  assert.match(quality, /data-t="[^"]+"/);
  assert.match(html, /id="qualitySteps"[^>]*min="1"[^>]*max="100"/);
  assert.match(html, /id="qualityWidth"[^>]*min="256"[^>]*max="4096"[^>]*step="32"/);
  assert.match(html, /id="qualityHeight"[^>]*min="256"[^>]*max="4096"[^>]*step="32"/);
  assert.match(html, /data-i18n="qualitySize"/);
  assert.match(html, /class="cfg-seed-card"[\s\S]*id="seed"/);
  assert.match(html, /class="seed-col divider"/);
  assert.doesNotMatch(html, /data-i18n="cfgHint"/);
  for (const aspect of ["1:1", "4:3", "3:4", "3:2", "2:3", "16:9", "9:16"]) {
    assert.match(html, new RegExp(`data-ar="${aspect}"`));
  }
});

test("collapse state persists and the selected preset shows one meta line", async () => {
  const { api, nodes, values } = setup();
  await api.init({ mode: "text", aspect: "1:1" });
  assert.equal(nodes.qualityPresets.open, false);
  assert.match(nodes.qualityPresetMeta.textContent, /1\.6 MP · 30 · ~1\.5 min/);
  nodes.qualityPresets.open = true;
  nodes.qualityPresets.listeners.toggle();
  assert.equal(values.get("quality.open"), "1");

  const restored = setup({ saved: { "quality.open": "1" } });
  await restored.api.init({ mode: "text", aspect: "1:1" });
  assert.equal(restored.nodes.qualityPresets.open, true);
});

test("preset selection resolves shared settings and manual edits switch to Custom", async () => {
  const { api, nodes, calls } = setup();
  await api.init({ mode: "text", aspect: "1:1" });
  assert.deepEqual(api.snapshot(), {
    preset: "standard", steps: 30, width: 1280, height: 1280,
    custom: false, valid: true, pixelBudget: 1_638_400,
  });
  await api.selectPreset("draft");
  assert.equal(api.snapshot().steps, 20);
  assert.equal(api.snapshot().width, 1024);
  assert.equal(calls.at(-1).command, "generation_settings_resolve");
  nodes.qualitySteps.value = "17";
  nodes.qualitySteps.listeners.input();
  assert.equal(api.snapshot().custom, true);
  assert.equal(api.snapshot().preset, "draft");
  assert.match(nodes.qualitySummary.textContent, /qualityCustom/);
  assert.equal(nodes.qualityPresetDraft.attributes["aria-checked"], "false");
  await api.selectPreset("max");
  assert.equal(api.snapshot().custom, false);
  assert.equal(api.snapshot().width, 2048);
  assert.equal(nodes.qualityPresetMax.attributes["aria-checked"], "true");
});

test("format changes preserve pixel area and round companion dimensions to 32", async () => {
  const { api, nodes } = setup();
  await api.init({ mode: "text", aspect: "1:1" });
  await api.setContext({ mode: "text", aspect: "3:4" });
  assert.deepEqual([api.snapshot().width, api.snapshot().height], [1120, 1472]);
  assert.equal(api.snapshot().pixelBudget, 1_648_640);
  assert.match(nodes.qualityStatus.textContent, /1\.6 MP/);
  assert.match(nodes.qualitySummary.textContent, /pStandard · 30 · 1120×1472/);
});

test("editing one dimension recalculates its paired side from the selected format", async () => {
  const { api, nodes } = setup();
  await api.init({ mode: "text", aspect: "1:1" });
  await api.setContext({ mode: "text", aspect: "3:4" });
  nodes.qualityWidth.value = "1024";
  nodes.qualityWidth.listeners.input();
  assert.deepEqual([api.snapshot().width, api.snapshot().height], [1024, 1376]);
  assert.equal(api.snapshot().custom, true);
});

test("sub-1024 custom sizes show a non-blocking detail warning", async () => {
  const { api, nodes } = setup();
  await api.init({ mode: "text", aspect: "1:1" });
  nodes.qualityWidth.value = "512";
  nodes.qualityWidth.listeners.input();
  assert.equal(api.snapshot().valid, true);
  assert.equal(nodes.qualityWarning.hidden, false);
  assert.match(nodes.qualityWarning.textContent, /qualityLowResolution/);
});

test("invalid manual values block snapshot submission and retain the last valid size", async () => {
  const { api, nodes } = setup();
  await api.init({ mode: "text", aspect: "1:1" });
  const last = api.snapshot();
  nodes.qualityWidth.value = "257";
  nodes.qualityWidth.listeners.input();
  assert.equal(api.snapshot().valid, false);
  assert.deepEqual([api.snapshot().width, api.snapshot().height], [last.width, last.height]);
  assert.notEqual(nodes.qualityError.textContent, "");
  nodes.qualityWidth.value = "";
  nodes.qualityWidth.listeners.input();
  assert.equal(api.snapshot().valid, false);
  nodes.qualityWidth.value = "4096";
  nodes.qualityWidth.listeners.input();
  assert.equal(api.snapshot().valid, false);
});

test("switching modes preserves the custom canvas instead of resetting to preset defaults", async () => {
  const { api, nodes } = setup({ invoke: async (_command, { request }) => {
    if (request.mode === "photos") return { steps: request.steps ?? 30, width: 1472, height: 1120 };
    return { steps: request.steps ?? 30, width: request.width ?? 1280, height: request.height ?? 1280 };
  } });
  await api.init({ mode: "text", aspect: "1:1" });
  nodes.qualityWidth.value = "1024";
  nodes.qualityWidth.listeners.input();
  assert.equal(api.snapshot().custom, true);

  await api.setContext({ mode: "text", aspect: "1:1", sourcePath: null });
  assert.deepEqual([api.snapshot().width, api.snapshot().height], [1024, 1024]);

  await api.setContext({ mode: "surprise", aspect: "random", sourcePath: null });
  assert.equal(api.snapshot().width, undefined);
  await api.setContext({ mode: "surprise", aspect: "1:1", sourcePath: null });
  assert.deepEqual([api.snapshot().width, api.snapshot().height], [1024, 1024]);

  await api.setContext({ mode: "photos", aspect: "1:1", sourcePath: "source.jpg" });
  assert.deepEqual([api.snapshot().width, api.snapshot().height], [1472, 1120]);
  await api.setContext({ mode: "text", aspect: "1:1", sourcePath: null });
  assert.deepEqual([api.snapshot().width, api.snapshot().height], [1024, 1024]);
  assert.equal(api.snapshot().custom, true);
});

test("photo dimensions are source-derived and read-only while steps stay editable", async () => {
  const { api, nodes, calls } = setup({ invoke: async (_command, { request }) => ({
    steps: request.steps ?? 30, width: request.images?.length ? 1472 : 1280,
    height: request.images?.length ? 1120 : 1280,
  }) });
  await api.init({ mode: "photos", aspect: "1:1", sourcePath: "source.jpg" });
  assert.equal(nodes.qualityWidth.disabled, true);
  assert.equal(nodes.qualityHeight.disabled, true);
  assert.equal(nodes.qualitySteps.disabled, false);
  assert.deepEqual([api.snapshot().width, api.snapshot().height], [1472, 1120]);
  assert.match(nodes.qualityStatus.textContent, /qualitySourceShort/);
  assert.match(nodes.qualityPresetMeta.textContent, /1280 px · 30/);
  assert.deepEqual(calls[0].args.request.images, ["source.jpg"]);
});

test("random Surprise keeps a pixel budget but leaves dimensions unresolved", async () => {
  const { api, nodes } = setup();
  await api.init({ mode: "surprise", aspect: "random" });
  assert.equal(nodes.qualityWidth.disabled, true);
  assert.equal(nodes.qualityHeight.disabled, true);
  assert.equal(nodes.qualitySizePair.hidden, true);
  assert.equal(nodes.qualitySizeAuto.hidden, false);
  assert.equal(nodes.qualityStatus.hidden, true);
  assert.equal(api.snapshot().width, undefined);
  assert.ok(api.snapshot().pixelBudget > 0);
});

test("storage migrations, corrupt values, and stale resolutions are safe", async () => {
  const corrupt = setup({ saved: { "quality.settings": "{broken" } });
  await corrupt.api.init({ mode: "text", aspect: "1:1" });
  assert.equal(corrupt.api.snapshot().preset, "standard");

  const legacy = setup({ saved: { preset: "draft" } });
  await legacy.api.init({ mode: "text", aspect: "1:1" });
  assert.equal(legacy.api.snapshot().preset, "draft");
  assert.equal(legacy.api.snapshot().steps, 20);

  const pending = [];
  const stale = setup({ invoke: (_command, args) => new Promise((resolve) => pending.push({ args, resolve })) });
  const init = stale.api.init({ mode: "text", aspect: "1:1" });
  pending.shift().resolve({ steps: 30, width: 1280, height: 1280 });
  await init;
  const draft = stale.api.selectPreset("draft");
  const max = stale.api.selectPreset("max");
  pending.shift().resolve({ steps: 20, width: 1024, height: 1024 });
  pending.shift().resolve({ steps: 40, width: 2048, height: 2048 });
  await Promise.all([draft, max]);
  assert.equal(stale.api.snapshot().preset, "max");
  assert.equal(stale.api.snapshot().width, 2048);
});

test("app snapshots resolved controls once per batch and excludes photo dimensions", () => {
  const app = readFileSync(new URL("../app.js", import.meta.url), "utf8");
  const masked = readFileSync(new URL("../masked-edit-run.js", import.meta.url), "utf8");
  assert.match(app, /steps:\s*state\.qualitySettings\.steps/);
  assert.match(app, /width:\s*state\.qualitySettings\.width/);
  assert.match(app, /height:\s*state\.qualitySettings\.height/);
  assert.match(app, /width:\s*snapshot\.mode === "photos" \? null : snapshot\.width/);
  assert.match(app, /const snapshot = \{[\s\S]*?steps:\s*state\.qualitySettings\.steps/);
  assert.match(app, /steps:\s*r\.steps/);
  assert.match(masked, /steps:\s*deps\.getSteps\?\.\(\)/);
});
