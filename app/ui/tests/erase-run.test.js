const assert = require("node:assert/strict");
const test = require("node:test");
const { create } = require("../erase-run.js");

const strings = { maskBusy: "busy", maskEmpty: "empty", cancelled: "cancelled", erase: "Erase" };

function setup({ invoke } = {}) {
  const state = {
    busy: false,
    generationRun: 0,
    stopRequested: false,
    history: [],
    sessionOutputs: new Set(),
  };
  const calls = { begin: 0, end: 0, applied: [], notices: [] };
  const flow = create({
    invoke,
    state,
    translate: (key) => strings[key] ?? key,
    hooks: {
      begin: () => { calls.begin += 1; },
      end: () => { calls.end += 1; },
      applyResult: (item) => calls.applied.push(item),
      notify: (text, ok) => calls.notices.push([text, ok]),
    },
  });
  return { state, calls, flow };
}

const payload = { sourcePath: "C:\\photos\\a.jpg", maskPngBase64: "AAAA" };
const backendResult = { path: "C:\\out\\erased.png", width: 1560, height: 1040, seconds: 3.5 };

test("erase sends only the source and mask, never a prompt or seed", async () => {
  const seen = [];
  const { flow } = setup({
    invoke: async (command, args) => { seen.push([command, args]); return backendResult; },
  });

  await flow.run(payload);

  assert.deepEqual(seen, [["erase_image", { args: { sourcePath: "C:\\photos\\a.jpg", maskPngBase64: "AAAA" } }]]);
});

test("a successful erase adds one comparison-ready history item", async () => {
  const { flow, state, calls } = setup({ invoke: async () => backendResult });

  const outcome = await flow.run(payload);

  assert.equal(outcome.ok, true);
  assert.equal(state.history.length, 1);
  assert.equal(calls.applied.length, 1);
  assert.equal(state.history[0], calls.applied[0]);
  assert.equal(state.history[0].path, backendResult.path);
  assert.equal(state.history[0].meta.mode, "erase");
  assert.equal(state.history[0].meta.prompt, "Erase");
  assert.equal(state.history[0].meta.model, "LaMa");
  assert.deepEqual(state.history[0].meta.references, ["C:\\photos\\a.jpg"]);
  assert.equal("seed" in state.history[0].meta, false);
  assert.ok(state.sessionOutputs.has(backendResult.path));
});

test("busy state brackets the erase and is released afterwards", async () => {
  let duringRun;
  const { flow, state, calls } = setup({
    invoke: async () => { duringRun = { busy: state.busy, begin: calls.begin, end: calls.end }; return backendResult; },
  });

  await flow.run(payload);

  assert.deepEqual(duringRun, { busy: true, begin: 1, end: 0 });
  assert.equal(state.busy, false);
  assert.equal(calls.end, 1);
});

test("a failed erase leaves the history alone and returns the error", async () => {
  const { flow, state, calls } = setup({ invoke: async () => { throw "The engine is off."; } });

  const outcome = await flow.run(payload);

  assert.deepEqual(outcome, { error: "The engine is off." });
  assert.equal(state.history.length, 0);
  assert.equal(calls.applied.length, 0);
  assert.equal(state.busy, false);
});

test("a cancelled erase or an invalidated run is not added to history", async () => {
  const cancelled = setup({ invoke: async () => { throw "Cancelled"; } });
  assert.deepEqual(await cancelled.flow.run(payload), { error: "cancelled" });
  assert.equal(cancelled.state.history.length, 0);
  assert.deepEqual(cancelled.calls.notices, [["cancelled", true]]);

  const invalidated = setup({
    invoke: async () => {
      invalidated.state.generationRun += 1;
      return backendResult;
    },
  });
  assert.deepEqual(await invalidated.flow.run(payload), { error: "cancelled" });
  assert.equal(invalidated.state.history.length, 0);
});

test("a second erase while busy is rejected without calling the backend", async () => {
  let calls = 0;
  const { flow, state } = setup({
    invoke: async () => { calls += 1; return backendResult; },
  });
  state.busy = true;

  assert.deepEqual(await flow.run(payload), { error: "busy" });
  assert.equal(calls, 0);
});

test("empty source or mask is rejected before the backend", async () => {
  let calls = 0;
  const { flow } = setup({ invoke: async () => { calls += 1; return backendResult; } });

  assert.deepEqual(await flow.run({ sourcePath: "", maskPngBase64: "AAAA" }), { error: "empty" });
  assert.deepEqual(await flow.run({ sourcePath: "C:\\a.jpg", maskPngBase64: "" }), { error: "empty" });
  assert.equal(calls, 0);
});
