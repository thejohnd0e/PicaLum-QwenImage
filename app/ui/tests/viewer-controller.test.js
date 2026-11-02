const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");
const viewerApi = require("../viewer.js");

class Element {
  constructor(id) {
    this.id = id;
    this.hidden = false;
    this.disabled = false;
    this.style = {};
    this.attributes = {};
    this.listeners = {};
    this.naturalWidth = 768;
    this.naturalHeight = 1024;
    this.clientWidth = 768;
    this.clientHeight = 1024;
    this.classList = {
      values: new Set(),
      toggle: (name, force) => force ? this.classList.values.add(name) : this.classList.values.delete(name),
      add: (name) => this.classList.values.add(name),
      remove: (name) => this.classList.values.delete(name),
      contains: (name) => this.classList.values.has(name),
    };
  }
  addEventListener(name, fn) { (this.listeners[name] ||= []).push(fn); }
  dispatch(name, event = {}) {
    event.currentTarget ||= this;
    event.target ||= this;
    event.preventDefault ||= () => { event.defaultPrevented = true; };
    for (const fn of this.listeners[name] || []) fn(event);
    return event;
  }
  setAttribute(name, value) { this.attributes[name] = value; }
  getAttribute(name) { return this.attributes[name]; }
  setPointerCapture(id) { this.captured = id; }
  hasPointerCapture(id) { return this.captured === id; }
  releasePointerCapture(id) { if (this.captured === id) this.captured = null; }
  closest() { return this; }
  getBoundingClientRect() { return { left: 0, width: this.clientWidth }; }
}

function setup() {
  const elements = new Map();
  const get = (id) => {
    if (!elements.has(id)) elements.set(id, new Element(id));
    return elements.get(id);
  };
  const windowListeners = {};
  const window = {
    QilViewer: viewerApi,
    addEventListener(name, fn) { (windowListeners[name] ||= []).push(fn); },
    dispatch(name, event = {}) { for (const fn of windowListeners[name] || []) fn(event); },
  };
  class ResizeObserver { observe() {} }
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, "../viewer-controller.js"), "utf8"), { window, QilViewer: viewerApi, ResizeObserver });
  const state = { compareMode: "result", compareZoom: "fit", comparePan: { x: 0.5, y: 0.5 }, wipePosition: 50 };
  const controller = window.QilComparison.create({ state, getElement: get, fileSrc: (value) => value, translate: (key) => key });
  const item = { path: "edited.png", meta: { mode: "photos", references: ["original.png"] } };
  controller.select(item);
  for (const id of ["beforeShot", "afterShot", "wipeBefore", "wipeAfter", "holdBefore", "holdAfter"]) get(id).onload?.();
  return { controller, state, get, item, window };
}

test("masked edit results expose comparison modes against the source, not the content reference", async () => {
  const { controller, state, get } = setup();
  const run = require("../masked-edit-run.js").create({
    state: { busy: false, generationRun: 0, preset: "standard", sessionOutputs: new Set(), history: [] },
    invoke: async () => ({ path: "masked.png", width: 1200, height: 900, seed: 1, steps: 30 }),
    translate: (key) => key, lockedSeed: () => 1, getCfg: () => 1,
    hooks: { begin() {}, end() {}, applyResult: (item) => controller.select(item), notify() {} },
  });
  await run.run({ sourcePath: "source.png", references: ["content.png"], instruction: "edit", maskPngBase64: "mask", featherPx: 8 });
  assert.equal(get("viewerToolbar").hidden, false);
  assert.equal(get("compareControls").hidden, false);
  assert.equal(get("beforeShot").src, "source.png");
  assert.equal(get("afterShot").src, "masked.png");
  for (const [button, mode] of [["viewSideBySide", "sideBySide"], ["viewWipe", "wipe"], ["viewHold", "hold"]]) {
    get(button).dispatch("click");
    assert.equal(state.compareMode, mode);
    assert.equal(get("compareView").hidden, false);
  }
  controller.reset();
  assert.equal(get("viewerToolbar").hidden, true);
});

test("hold mode ships a visible instruction inside the stage", () => {
  const html = fs.readFileSync(path.join(__dirname, "../index.html"), "utf8");
  assert.match(html, /id="holdStage"[\s\S]*?id="holdHint"[\s\S]*?data-i18n="compareHoldHint"/);
});

test("holding the Hold button reveals the original until the pointer is released", () => {
  const { state, get } = setup();
  get("viewHold").dispatch("pointerdown", { button: 0, pointerId: 11 });
  assert.equal(state.compareMode, "hold");
  assert.equal(get("holdBefore").hidden, false);
  assert.equal(get("holdAfter").hidden, true);
  assert.equal(get("viewHold").hasPointerCapture(11), true);
  get("viewHold").dispatch("pointerup", { pointerId: 11 });
  assert.equal(get("holdBefore").hidden, true);
  assert.equal(get("holdAfter").hidden, false);
  assert.equal(get("viewHold").hasPointerCapture(11), false);
});

test("Space reveals the original while the Hold button keeps focus", () => {
  const { get } = setup();
  get("viewHold").dispatch("click");
  const down = get("viewer").dispatch("keydown", { key: " ", target: get("viewHold") });
  assert.equal(down.defaultPrevented, true);
  assert.equal(get("holdBefore").hidden, false);
  get("viewer").dispatch("keyup", { key: " " });
  assert.equal(get("holdBefore").hidden, true);
  assert.equal(get("holdAfter").hidden, false);
});

test("erase results compare against their source", () => {
  const { controller, get } = setup();
  controller.select({
    path: "erased.png",
    meta: { mode: "erase", references: ["source.png"] },
  });
  assert.equal(get("viewerToolbar").hidden, false);
  assert.equal(get("compareControls").hidden, false);
  assert.equal(get("beforeShot").src, "source.png");
  assert.equal(get("afterShot").src, "erased.png");
});

test("hold mode shows the original only during primary-pointer press and restores result on release or cancellation", () => {
  const { state, get } = setup();
  get("viewHold").dispatch("click");
  assert.equal(state.compareMode, "hold");
  assert.equal(get("holdAfter").hidden, false);
  assert.equal(get("holdBefore").hidden, true);

  const down = get("holdStage").dispatch("pointerdown", { button: 0, pointerId: 7 });
  assert.equal(get("holdBefore").hidden, false);
  assert.equal(get("holdAfter").hidden, true);
  assert.equal(get("holdStage").hasPointerCapture(7), true);

  get("holdStage").dispatch("pointerup", { pointerId: 7 });
  assert.equal(get("holdBefore").hidden, true);
  assert.equal(get("holdAfter").hidden, false);
  assert.equal(get("holdStage").hasPointerCapture(7), false);
  assert.equal(down.defaultPrevented, true);

  get("holdStage").dispatch("pointerdown", { button: 0, pointerId: 8 });
  get("holdStage").dispatch("pointercancel", { pointerId: 8 });
  assert.equal(get("holdBefore").hidden, true);
  assert.equal(get("holdAfter").hidden, false);
});

test("hold mode ignores non-primary pointer, Space is scoped to the viewer, and blur restores the result", () => {
  const { state, get, window } = setup();
  get("viewHold").dispatch("click");
  get("holdStage").dispatch("pointerdown", { button: 2, pointerId: 2 });
  assert.equal(get("holdBefore").hidden, true);

  const typing = get("viewer").dispatch("keydown", { key: " ", target: { tagName: "INPUT" } });
  assert.equal(typing.defaultPrevented, undefined);
  const editable = get("viewer").dispatch("keydown", { key: " ", target: { tagName: "DIV", isContentEditable: true } });
  assert.equal(editable.defaultPrevented, undefined);
  assert.equal(get("holdBefore").hidden, true);

  const space = get("viewer").dispatch("keydown", { key: " ", target: get("viewer") });
  assert.equal(space.defaultPrevented, true);
  assert.equal(get("holdBefore").hidden, false);
  get("viewer").dispatch("keyup", { key: " ", target: get("viewer") });
  assert.equal(get("holdBefore").hidden, true);

  get("holdStage").dispatch("pointerdown", { button: 0, pointerId: 9 });
  window.dispatch("blur");
  assert.equal(get("holdBefore").hidden, true);
  assert.equal(get("holdAfter").hidden, false);
  assert.equal(get("holdStage").hasPointerCapture(9), false);
  assert.equal(state.compareMode, "hold");
});

test("changing item, leaving hold mode, or losing a comparison source always restores the result", () => {
  const { controller, state, get, item } = setup();
  get("viewHold").dispatch("click");
  get("holdStage").dispatch("pointerdown", { button: 0, pointerId: 3 });
  controller.select({ ...item, path: "another.png" });
  assert.equal(state.compareMode, "result");
  assert.equal(get("holdBefore").hidden, true);

  get("viewHold").dispatch("click");
  get("holdStage").dispatch("pointerdown", { button: 0, pointerId: 4 });
  get("viewWipe").dispatch("click");
  assert.equal(state.compareMode, "wipe");
  assert.equal(get("holdBefore").hidden, true);

  controller.select({ path: "text.png", meta: { mode: "text", references: [] } });
  assert.equal(state.compareMode, "result");
  assert.equal(get("holdBefore").hidden, true);
});

test("hold mode preserves the shared zoom and pan transforms", () => {
  const { state, get } = setup();
  get("zoom100").dispatch("click");
  state.comparePan = { x: 0.2, y: 0.8 };
  get("viewHold").dispatch("click");
  const beforeTransform = get("holdBefore").style.transform;
  const afterTransform = get("holdAfter").style.transform;
  assert.equal(beforeTransform, afterTransform);
  assert.match(afterTransform, /scale\(1\)/);
  assert.deepEqual(state.comparePan, { x: 0.2, y: 0.8 });
});
