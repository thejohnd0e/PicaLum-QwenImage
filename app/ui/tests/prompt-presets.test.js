const test = require("node:test");
const assert = require("node:assert/strict");
const { readFileSync } = require("node:fs");
const { join } = require("node:path");
const { create } = require("../prompt-presets.js");

class Element {
  constructor(tag = "div") {
    this.tagName = tag.toUpperCase();
    this.children = [];
    this.listeners = {};
    this.attributes = {};
    this.dataset = {};
    this.value = "";
    this.textContent = "";
    this.disabled = false;
    this.hidden = false;
    this.className = "";
  }
  appendChild(child) { this.children.push(child); return child; }
  replaceChildren(...children) { this.children = children; }
  add(option) { this.appendChild(option); }
  addEventListener(type, fn) { (this.listeners[type] ||= []).push(fn); }
  setAttribute(name, value) { this.attributes[name] = String(value); }
  getAttribute(name) { return this.attributes[name] ?? null; }
  focus() { global.document.activeElement = this; }
  querySelectorAll() { return this.children.filter((child) => child.tagName === "BUTTON" || child.tagName === "INPUT" || child.tagName === "TEXTAREA" || child.tagName === "SELECT"); }
  async emit(type, event = {}) { for (const fn of this.listeners[type] || []) await fn({ target: this, preventDefault() {}, ...event }); }
}

const ids = [
  "promptPresetSelect", "promptPresetManage", "promptPresetsBackdrop", "promptPresetsDialog",
  "promptPresetsList", "promptPresetName", "promptPresetPrompt", "promptPresetNew",
  "promptPresetDuplicate", "promptPresetDelete", "promptPresetFromCurrent", "promptPresetsSave",
  "promptPresetsCancel", "promptPresetsError", "promptPresetsRetry",
  "promptPhotos",
];

function setup({ initial = [], load, save = async (_name, { document }) => document, busy = () => false, translate } = {}) {
  const elements = Object.fromEntries(ids.map((id) => [id, new Element(id.endsWith("Select") ? "select" : "button")]));
  for (const [id, element] of Object.entries(elements)) element.id = id;
  elements.promptPresetName = new Element("input");
  elements.promptPresetPrompt = new Element("textarea");
  elements.promptPresetsBackdrop.hidden = true;
  elements.promptPresetsError.hidden = true;
  elements.promptPresetsRetry.hidden = true;
  elements.promptPresetsDialog.querySelectorAll = () => [
    ...elements.promptPresetsList.children,
    elements.promptPresetNew, elements.promptPresetDuplicate, elements.promptPresetDelete,
    elements.promptPresetName, elements.promptPresetPrompt, elements.promptPresetFromCurrent,
    elements.promptPresetsRetry, elements.promptPresetsCancel, elements.promptPresetsSave,
  ];
  global.document = { activeElement: null, createElement: (tag) => new Element(tag) };
  let persisted = structuredClone(initial);
  const errors = [];
  const applied = [];
  let currentPrompt = "";
  const controller = create({
    invoke: async (name, args) => {
      if (name === "prompt_presets_load") return load ? load() : structuredClone(persisted);
      if (name === "prompt_presets_save") {
        const result = await save(name, args);
        persisted = structuredClone(result);
        return structuredClone(result);
      }
      throw new Error(`unexpected command: ${name}`);
    },
    getElement: (id) => elements[id],
    translate: translate || ((key, ...args) => key === "qRemoveBg" ? "Remove background" : key === "promptPresetCopyName" ? `Copy ${args[0] || ""}` : key),
    getPrompt: () => currentPrompt,
    applyPrompt: (value) => applied.push(value),
    notify: (message) => errors.push(message),
    isBusy: busy,
  });
  return { controller, elements, errors, applied, persisted: () => structuredClone(persisted), setCurrentPrompt: (v) => { currentPrompt = v; } };
}

const documentWith = (...presets) => ({ version: 1, presets });
const item = (id, name, prompt, nameKey) => ({ id, name, prompt, ...(nameKey ? { nameKey } : {}) });

test("applying exact multiline prompt resets the selector for repeat application", async () => {
  const prompt = "Строка 1\n<image2>  ";
  const app = setup({ initial: documentWith(item("one", "One", prompt)) });
  await app.controller.load();
  app.elements.promptPresetSelect.value = "one";
  await app.elements.promptPresetSelect.emit("change");
  assert.deepEqual(app.applied, [prompt]);
  assert.equal(app.elements.promptPresetSelect.value, "");
  assert.equal(global.document.activeElement, app.elements.promptPhotos);
  app.elements.promptPresetSelect.value = "one";
  await app.elements.promptPresetSelect.emit("change");
  assert.deepEqual(app.applied, [prompt, prompt]);
});

test("cancel discards edits without changing the persisted document", async () => {
  const app = setup({ initial: documentWith(item("one", "One", "Original")) });
  await app.controller.load();
  await app.elements.promptPresetManage.emit("click");
  app.elements.promptPresetName.value = "Changed";
  await app.elements.promptPresetsCancel.emit("click");
  assert.deepEqual(app.persisted(), documentWith(item("one", "One", "Original")));
});

test("save failure keeps the editor draft and in-memory document", async () => {
  const app = setup({ initial: documentWith(item("one", "One", "Original")), save: async () => { throw new Error("disk full"); } });
  await app.controller.load();
  await app.elements.promptPresetManage.emit("click");
  app.elements.promptPresetName.value = "Changed";
  await app.elements.promptPresetsSave.emit("click");
  assert.equal(app.elements.promptPresetsBackdrop.hidden, false);
  assert.equal(app.elements.promptPresetName.value, "Changed");
  assert.equal(app.elements.promptPresetsError.hidden, false);
  assert.deepEqual(app.persisted(), documentWith(item("one", "One", "Original")));
});

test("successful save replaces the in-memory list and closes the editor", async () => {
  const app = setup({ initial: documentWith(item("one", "One", "Original")) });
  await app.controller.load();
  await app.elements.promptPresetManage.emit("click");
  await app.elements.promptPresetNew.emit("click");
  app.elements.promptPresetName.value = "New prompt";
  app.elements.promptPresetPrompt.value = "Keep exact\ntext";
  await app.elements.promptPresetsSave.emit("click");
  assert.equal(app.persisted().presets.length, 2);
  assert.equal(app.persisted().presets[1].prompt, "Keep exact\ntext");
  assert.equal(app.elements.promptPresetsBackdrop.hidden, true);
});

test("duplicate gets a fresh user ID and clears its built-in localization key", async () => {
  const app = setup({ initial: documentWith(item("built", "Built in", "Prompt", "qRemoveBg")) });
  await app.controller.load();
  await app.elements.promptPresetManage.emit("click");
  await app.elements.promptPresetDuplicate.emit("click");
  assert.match(app.elements.promptPresetName.value, /Remove background/);
  assert.equal(app.controller.isOpen(), true);
  await app.elements.promptPresetsSave.emit("click");
  const duplicate = app.persisted().presets[1];
  assert.match(duplicate.id, /^user-/);
  assert.equal("nameKey" in duplicate, false);
  assert.equal(duplicate.prompt, "Prompt");
});

test("renaming clears nameKey but a prompt-only edit keeps it", async () => {
  const first = setup({ initial: documentWith(item("built", "Built", "Old", "qRemoveBg")) });
  await first.controller.load();
  await first.elements.promptPresetManage.emit("click");
  first.elements.promptPresetPrompt.value = "New";
  await first.elements.promptPresetsSave.emit("click");
  assert.equal(first.persisted().presets[0].nameKey, "qRemoveBg");

  const second = setup({ initial: documentWith(item("built", "Built", "Old", "qRemoveBg")) });
  await second.controller.load();
  await second.elements.promptPresetManage.emit("click");
  second.elements.promptPresetName.value = "Renamed";
  await second.elements.promptPresetsSave.emit("click");
  assert.equal("nameKey" in second.persisted().presets[0], false);
});

test("load failure blocks saving and retrying through Manage loads the document", async () => {
  let attempt = 0;
  const app = setup({ load: async () => {
    if (attempt++ < 2) throw new Error("corrupt");
    return documentWith(item("one", "One", "Prompt"));
  } });
  await app.controller.load();
  assert.equal(app.errors.includes("promptPresetsLoadFailed"), true);
  await app.elements.promptPresetManage.emit("click");
  assert.equal(app.elements.promptPresetsSave.disabled, true);
  assert.equal(app.elements.promptPresetsRetry.hidden, false);
  await app.elements.promptPresetsRetry.emit("click");
  assert.equal(app.elements.promptPresetsSave.disabled, false);
  assert.equal(app.elements.promptPresetName.value, "One");
  assert.equal(app.controller.isOpen(), true);
});

test("empty preset library can be saved", async () => {
  const app = setup({ initial: documentWith() });
  await app.controller.load();
  await app.elements.promptPresetManage.emit("click");
  await app.elements.promptPresetsSave.emit("click");
  assert.deepEqual(app.persisted(), documentWith());
});

test("switching list entries flushes the current fields into the draft", async () => {
  const app = setup({ initial: documentWith(item("one", "One", "First"), item("two", "Two", "Second")) });
  await app.controller.load();
  await app.elements.promptPresetManage.emit("click");
  app.elements.promptPresetName.value = "Renamed one";
  app.elements.promptPresetPrompt.value = "Edited one";
  await app.elements.promptPresetsList.children[1].emit("click");
  assert.equal(app.elements.promptPresetName.value, "Two");
  await app.elements.promptPresetsList.children[0].emit("click");
  assert.equal(app.elements.promptPresetName.value, "Renamed one");
  assert.equal(app.elements.promptPresetPrompt.value, "Edited one");
});

test("language rerender localizes built-ins without changing prompt text or user names", async () => {
  let language = "English built-in";
  const app = setup({
    initial: documentWith(item("built", "Built", "Keep\nthis", "qRemoveBg"), item("user", "My own name", "User prompt")),
    translate: (key) => key === "qRemoveBg" ? language : key,
  });
  await app.controller.load();
  await app.elements.promptPresetManage.emit("click");
  app.elements.promptPresetPrompt.value = "Unsaved\ntext";
  language = "Nombre integrado";
  app.controller.render({ preserveForm: true });
  assert.equal(app.elements.promptPresetSelect.children[1].textContent, "Nombre integrado");
  assert.equal(app.elements.promptPresetSelect.children[2].textContent, "My own name");
  assert.equal(app.elements.promptPresetName.value, "Nombre integrado");
  assert.equal(app.elements.promptPresetPrompt.value, "Unsaved\ntext");
  app.elements.promptPresetName.value = "Uncommitted name";
  await app.elements.promptPresetName.emit("input");
  language = "Nombre local";
  app.controller.render({ preserveForm: true });
  assert.equal(app.elements.promptPresetName.value, "Uncommitted name");
});

test("busy generation blocks preset selection and editor opening", async () => {
  let isBusy = false;
  const app = setup({ initial: documentWith(item("one", "One", "Prompt")), busy: () => isBusy });
  await app.controller.load();
  isBusy = true;
  app.controller.setDisabled(true);
  app.elements.promptPresetSelect.value = "one";
  await app.elements.promptPresetSelect.emit("change");
  await app.elements.promptPresetManage.emit("click");
  assert.deepEqual(app.applied, []);
  assert.equal(app.controller.isOpen(), false);
});

test("From current prompt creates a preset while retaining the selected draft edits", async () => {
  const app = setup({ initial: documentWith(item("one", "One", "Original")) });
  app.setCurrentPrompt("Текущий\n<image2>");
  await app.controller.load();
  await app.elements.promptPresetManage.emit("click");
  app.elements.promptPresetName.value = "Edited name";
  app.elements.promptPresetPrompt.value = "Edited draft";
  await app.elements.promptPresetFromCurrent.emit("focus");
  assert.equal(app.elements.promptPresetName.value, "Edited name");
  await app.elements.promptPresetFromCurrent.emit("click");
  assert.equal(app.elements.promptPresetName.value, "promptPresetFromCurrent");
  assert.equal(app.elements.promptPresetPrompt.value, "Текущий\n<image2>");
  await app.elements.promptPresetsSave.emit("click");
  assert.deepEqual(app.persisted().presets.map(({ name, prompt }) => [name, prompt]), [
    ["Edited name", "Edited draft"],
    ["promptPresetFromCurrent", "Текущий\n<image2>"],
  ]);
});

test("Escape cancels the editor and restores focus to Manage", async () => {
  const app = setup({ initial: documentWith(item("one", "One", "Prompt")) });
  await app.controller.load();
  await app.elements.promptPresetManage.emit("click");
  let prevented = false;
  app.controller.handleKeydown({ key: "Escape", preventDefault() { prevented = true; } });
  assert.equal(prevented, true);
  assert.equal(app.controller.isOpen(), false);
  assert.equal(global.document.activeElement, app.elements.promptPresetManage);
});

test("Ctrl+Enter is prevented while the editor remains open", async () => {
  const app = setup({ initial: documentWith(item("one", "One", "Prompt")) });
  await app.controller.load();
  await app.elements.promptPresetManage.emit("click");
  let prevented = false;
  app.controller.handleKeydown({ key: "Enter", ctrlKey: true, preventDefault() { prevented = true; } });
  assert.equal(prevented, true);
  assert.equal(app.controller.isOpen(), true);
});

test("Tab and Shift+Tab wrap focus within the dialog", async () => {
  const app = setup({ initial: documentWith(item("one", "One", "Prompt")) });
  await app.controller.load();
  await app.elements.promptPresetManage.emit("click");
  const first = app.elements.promptPresetsList.children[0];
  const last = app.elements.promptPresetsSave;
  global.document.activeElement = first;
  app.controller.handleKeydown({ key: "Tab", shiftKey: true, preventDefault() {} });
  assert.equal(global.document.activeElement, last, "Shift+Tab from the first dialog control should wrap to Save");
  app.controller.handleKeydown({ key: "Tab", shiftKey: false, preventDefault() {} });
  assert.equal(global.document.activeElement, first, "Tab from Save should wrap to the first dialog control");
});

test("prompt input keeps an accessible name and the editor is an announced modal dialog", () => {
  const html = readFileSync(join(__dirname, "..", "index.html"), "utf8");
  assert.match(html, /id="promptText"[^>]*data-i18n-aria="description"/);
  assert.doesNotMatch(html, /<label[^>]+for="promptText"/);
  assert.match(html, /id="promptPresetsDialog"[^>]*role="dialog"[^>]*aria-modal="true"[^>]*aria-labelledby="promptPresetsHeading"/);
  assert.match(html, /id="promptPresetsError" role="alert" aria-live="polite"/);
});
