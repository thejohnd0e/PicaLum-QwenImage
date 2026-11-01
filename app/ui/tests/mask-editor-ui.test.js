const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const test = require("node:test");

// Use only IDs shipped in the actual page: absent controls must return null,
// as in a browser, rather than being silently invented by a DOM test double.
function setup() {
  const html = fs.readFileSync(path.join(__dirname, "../index.html"), "utf8");
  const nodes = new Map();
  function element() {
    return {
      hidden: false, value: "", style: {}, clientWidth: 800, clientHeight: 450,
      listeners: {}, textContent: "", isConnected: true,
      setAttribute() {}, focus() {}, appendChild() {},
      addEventListener(type, fn) { this.listeners[type] = fn; },
      getContext() { return {}; },
    };
  }
  for (const match of html.matchAll(/id="([^"]+)"/g)) nodes.set(match[1], element());
  nodes.get("maskBackdrop").hidden = true;
  nodes.get("maskFeather").value = "8";
  const images = [];
  const window = { addEventListener() {}, devicePixelRatio: 1 };
  const context = vm.createContext({
    window,
    document: { createElement: element, addEventListener() {}, activeElement: null },
    Image: class { constructor() { images.push(this); } },
    ResizeObserver: class { observe() {} disconnect() {} },
    requestAnimationFrame: () => 1,
    setTimeout, clearTimeout,
  });
  for (const file of ["mask-editor.js", "mask-preview.js", "mask-editor-ui.js"]) {
    vm.runInContext(fs.readFileSync(path.join(__dirname, "..", file), "utf8"), context, { filename: file });
  }
  const editor = window.QilMaskEditorUi.create({
    getElement: (id) => nodes.get(id) || null,
    translate: (key) => key,
    fileSrc: (p) => p,
    getSourcePath: () => "photo.jpg",
    getPrompt: () => "Edit photo",
    getReferences: () => [],
    notify() {},
  });
  return { editor, nodes, images };
}

test("area editor opens with the shipped HTML, loads the photo, and closes", async () => {
  const { editor, nodes, images } = setup();
  await editor.open();
  assert.equal(editor.isOpen(), true);
  assert.equal(images.length, 1, "opening must reach image loading");
  const image = images[0];
  image.naturalWidth = 3088;
  image.naturalHeight = 2320;
  image.onload();
  assert.equal(nodes.get("maskLoading").hidden, true);
  assert.equal(nodes.get("maskBrush").disabled, false);
  assert.equal(nodes.get("maskClose").textContent, "maskClose");
  nodes.get("maskClose").listeners.click();
  assert.equal(editor.isOpen(), false);
});

test("area editor can close while photo loading is still pending", async () => {
  const { editor, nodes } = setup();
  await editor.open();
  nodes.get("maskClose").listeners.click();
  assert.equal(editor.isOpen(), false);
});
