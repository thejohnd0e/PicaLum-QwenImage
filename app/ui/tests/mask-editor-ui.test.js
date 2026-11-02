const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const test = require("node:test");

// Use only IDs shipped in the actual page: absent controls must return null,
// as in a browser, rather than being silently invented by a DOM test double.
function setup({
  cfg = 2.3,
  submit = async () => ({ ok: true }),
  submitErase = async () => ({ ok: true }),
  lamaStatus = { state: "ready" },
  lamaDownload = async () => {},
  listen,
  canRun = () => null,
} = {}) {
  const html = fs.readFileSync(path.join(__dirname, "../index.html"), "utf8");
  const nodes = new Map();
  function element() {
    return {
      hidden: false, value: "", style: {}, clientWidth: 800, clientHeight: 450,
      validityMessage: "",
      setCustomValidity(message) { this.validityMessage = message; },
      checkValidity() {
        const value = Number(this.value);
        return !this.validityMessage && this.value !== "" && Number.isFinite(value)
          && (!this.min || value >= Number(this.min)) && (!this.max || value <= Number(this.max));
      },
      listeners: {}, textContent: "", isConnected: true,
      setAttribute() {}, focus() {}, appendChild() {},
      reportValidity() {},
      setPointerCapture() {},
      getBoundingClientRect() { return { left: 0, top: 0 }; },
      toBlob(callback) { callback({ arrayBuffer: async () => new Uint8Array([137, 80, 78, 71]).buffer }); },
      addEventListener(type, fn) { this.listeners[type] = fn; },
      getContext() {
        return {
          save() {}, restore() {}, beginPath() {}, moveTo() {}, lineTo() {}, stroke() {},
          clearRect() {},
          createImageData(w, h) { return { data: new Uint8ClampedArray(w * h * 4) }; },
          putImageData() {},
        };
      },
    };
  }
  for (const match of html.matchAll(/<[^>]+\bid="([^"]+)"[^>]*>/g)) {
    const node = element();
    for (const attr of ["min", "max", "step", "value"]) {
      const value = match[0].match(new RegExp(`\\b${attr}="([^"]+)"`));
      if (value) node[attr] = value[1];
    }
    nodes.set(match[1], node);
  }
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
    setTimeout: () => 1, clearTimeout() {}, btoa,
  });
  for (const file of ["mask-editor.js", "mask-preview.js", "mask-editor-ui.js"]) {
    vm.runInContext(fs.readFileSync(path.join(__dirname, "..", file), "utf8"), context, { filename: file });
  }
  const invoked = [];
  const editor = window.QilMaskEditorUi.create({
    getElement: (id) => nodes.get(id) || null,
    invoke: async (command, args) => {
      invoked.push([command, args]);
      if (command === "lama_status") return lamaStatus;
      if (command === "lama_download") return lamaDownload();
      return null;
    },
    listen,
    translate: (key) => key,
    fileSrc: (p) => p,
    getSourcePath: () => "photo.jpg",
    getPrompt: () => "Edit photo",
    getCfg: () => cfg,
    getReferences: () => [],
    canRun,
    submit,
    submitErase,
    notify() {},
  });
  return { editor, nodes, images, invoked };
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

test("area CFG starts from main window and synchronizes slider and numeric input", async () => {
  const { editor, nodes } = setup({ cfg: 2.3 });
  await editor.open();
  const range = nodes.get("maskCfg");
  const numeric = nodes.get("maskCfgNumber");
  assert.equal(range?.value, "2.3");
  assert.equal(numeric?.value, "2.3");
  assert.equal(range.min, "1.0");
  assert.equal(range.max, "5.0");
  assert.equal(range.step, "0.1");
  range.value = "3.7";
  range.listeners.input();
  assert.equal(numeric.value, "3.7");
  numeric.value = "4.2";
  numeric.listeners.input();
  assert.equal(range.value, "4.2");
});

test("invalid area CFG does not overwrite the valid slider and can be corrected", async () => {
  const { editor, nodes } = setup();
  await editor.open();
  const range = nodes.get("maskCfg");
  const numeric = nodes.get("maskCfgNumber");
  assert.ok(numeric, "CFG numeric input must be present in shipped HTML");
  for (const value of ["", "0.9", "5.1", "NaN"]) {
    numeric.value = value;
    numeric.listeners.input();
    assert.equal(numeric.checkValidity(), false);
    assert.equal(range.value, "2.3");
  }
  numeric.value = "1.0";
  numeric.listeners.input();
  assert.equal(numeric.checkValidity(), true);
  assert.equal(range.value, "1.0");
});

test("a missing LaMa model shows Download instead of Erase and never downloads on open", async () => {
  const { editor, nodes, invoked } = setup({ lamaStatus: { state: "missing" } });
  await editor.open();

  assert.equal(nodes.get("maskDownload").hidden, false);
  assert.equal(nodes.get("maskErase").hidden, true);
  assert.equal(nodes.get("maskDownload").textContent, "downloadLama");
  assert.equal(invoked.some(([command]) => command === "lama_download"), false);
  editor.handleKeydown({ key: "Escape", preventDefault() {}, target: nodes.get("maskDialog") });
});

test("a successful model download reveals Erase and reports progress", async () => {
  const status = { state: "missing" };
  let progress;
  let finishDownload;
  const { editor, nodes } = setup({
    lamaStatus: status,
    lamaDownload: () => new Promise((resolve) => {
      finishDownload = () => {
        status.state = "ready";
        resolve();
      };
    }),
    listen: async (name, callback) => {
      assert.equal(name, "lama-download-progress");
      progress = callback;
      return () => {};
    },
  });
  await editor.open();
  const pending = nodes.get("maskDownload").listeners.click();
  for (let i = 0; i < 10 && !finishDownload; i += 1) await Promise.resolve();
  progress({ payload: { done: 100, total: 200 } });
  assert.match(nodes.get("maskStatus").textContent, /50%/);
  finishDownload();
  await pending;

  assert.equal(nodes.get("maskErase").hidden, false);
  assert.equal(nodes.get("maskDownload").hidden, true);
  assert.equal(nodes.get("maskStatus").textContent, "lamaReady");
  editor.handleKeydown({ key: "Escape", preventDefault() {}, target: nodes.get("maskDialog") });
});

test("a failed model download keeps the Download button and shows the error", async () => {
  const { editor, nodes } = setup({
    lamaStatus: { state: "missing" },
    lamaDownload: async () => {
      throw "network down";
    },
  });
  await editor.open();
  await nodes.get("maskDownload").listeners.click();

  assert.equal(nodes.get("maskDownload").hidden, false);
  assert.equal(nodes.get("maskErase").hidden, true);
  assert.equal(nodes.get("maskError").hidden, false);
  assert.equal(nodes.get("maskError").textContent, "network down");
  editor.handleKeydown({ key: "Escape", preventDefault() {}, target: nodes.get("maskDialog") });
});

test("Erase sends the source and mask, ignores an empty instruction, and closes on success", async () => {
  let erased;
  let generated = false;
  const { editor, nodes, images } = setup({
    submitErase: async (payload) => {
      erased = payload;
      return { ok: true };
    },
    submit: async () => {
      generated = true;
      return { ok: true };
    },
  });
  await editor.open();
  images[0].naturalWidth = 8;
  images[0].naturalHeight = 8;
  images[0].onload();
  nodes.get("maskInstruction").value = "";
  nodes.get("maskCanvas").listeners.pointerdown({ button: 0, pointerId: 1, clientX: 4, clientY: 4 });
  await nodes.get("maskErase").listeners.click();

  assert.equal(generated, false);
  assert.equal(erased.sourcePath, "photo.jpg");
  assert.equal(typeof erased.maskPngBase64, "string");
  assert.ok(erased.maskPngBase64.length > 0);
  assert.equal(editor.isOpen(), false);
});

test("Erase refuses an empty mask before calling the backend", async () => {
  let calls = 0;
  const { editor, nodes, images } = setup({
    submitErase: async () => {
      calls += 1;
      return { ok: true };
    },
  });
  await editor.open();
  images[0].naturalWidth = 8;
  images[0].naturalHeight = 8;
  images[0].onload();
  await nodes.get("maskErase").listeners.click();

  assert.equal(calls, 0);
  assert.equal(nodes.get("maskError").textContent, "maskEmpty");
  editor.handleKeydown({ key: "Escape", preventDefault() {}, target: nodes.get("maskDialog") });
});

test("a late download completion cannot change a reopened dialog", async () => {
  const status = { state: "missing" };
  let finishDownload;
  const { editor, nodes } = setup({
    lamaStatus: status,
    lamaDownload: () => new Promise((resolve) => {
      finishDownload = () => {
        status.state = "ready";
        resolve();
      };
    }),
  });
  await editor.open();
  const pending = nodes.get("maskDownload").listeners.click();
  for (let i = 0; i < 10 && !finishDownload; i += 1) await Promise.resolve();
  nodes.get("maskClose").listeners.click();
  await editor.open();
  assert.equal(nodes.get("maskErase").hidden, true);
  finishDownload();
  await pending;
  assert.equal(nodes.get("maskErase").hidden, true);
  assert.equal(nodes.get("maskDownload").hidden, false);
  nodes.get("maskClose").listeners.click();
});

test("Erase reports an unavailable engine before calling the backend", async () => {
  let calls = 0;
  const { editor, nodes, images } = setup({
    canRun: () => "maskEngineOff",
    submitErase: async () => {
      calls += 1;
      return { ok: true };
    },
  });
  await editor.open();
  images[0].naturalWidth = 8;
  images[0].naturalHeight = 8;
  images[0].onload();
  nodes.get("maskCanvas").listeners.pointerdown({ button: 0, pointerId: 1, clientX: 4, clientY: 4 });
  await nodes.get("maskErase").listeners.click();

  assert.equal(calls, 0);
  assert.equal(nodes.get("maskError").textContent, "maskEngineOff");
  editor.handleKeydown({ key: "Escape", preventDefault() {}, target: nodes.get("maskDialog") });
});

test("area generation sends the edited CFG and blocks an invalid numeric value", async () => {
  let sent;
  const { editor, nodes, images } = setup({ submit: async (payload) => { sent = payload; return { ok: true }; } });
  await editor.open();
  images[0].naturalWidth = 8;
  images[0].naturalHeight = 8;
  images[0].onload();
  nodes.get("maskCanvas").listeners.pointerdown({ button: 0, pointerId: 1, clientX: 4, clientY: 4 });
  const numeric = nodes.get("maskCfgNumber");
  numeric.value = "5.1";
  numeric.listeners.input();
  await nodes.get("maskGenerate").listeners.click();
  assert.equal(sent, undefined);
  numeric.value = "3.7";
  numeric.listeners.input();
  await nodes.get("maskGenerate").listeners.click();
  assert.equal(sent.cfg, 3.7);
  assert.equal(editor.isOpen(), false);
});
