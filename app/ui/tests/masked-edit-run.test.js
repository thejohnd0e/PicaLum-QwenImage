const assert = require("node:assert/strict");
const test = require("node:test");
const { create } = require("../masked-edit-run.js");

const strings = { maskBusy: "busy", cancelled: "cancelled" };

function setup({ invoke, lockedSeed = null } = {}) {
  const state = {
    busy: false,
    preset: "standard",
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
    lockedSeed: () => lockedSeed,
    hooks: {
      begin: () => { calls.begin += 1; },
      end: () => { calls.end += 1; },
      applyResult: (item) => calls.applied.push(item),
      notify: (text, ok) => calls.notices.push([text, ok]),
    },
  });
  return { state, calls, flow };
}

const payload = {
  sourcePath: "C:\\photos\\a.jpg",
  maskPngBase64: "AAAA",
  instruction: "Make the saddle brown.",
  featherPx: 8,
};
const backendResult = { path: "C:\\out\\masked.png", seed: 42, width: 1560, height: 1040, seconds: 12.5 };

test("the request carries the exact source, mask, instruction, preset, seed and references", async () => {
  const seen = [];
  const { flow } = setup({
    lockedSeed: 777,
    invoke: async (command, args) => { seen.push([command, args]); return backendResult; },
  });

  await flow.run({ ...payload, references: ["C:\\photos\\ref.jpg"] });

  assert.deepEqual(seen, [["generate_masked_edit", {
    args: {
      sourcePath: "C:\\photos\\a.jpg",
      maskPngBase64: "AAAA",
      instruction: "Make the saddle brown.",
      preset: "standard",
      seed: 777,
      featherPx: 8,
      references: ["C:\\photos\\ref.jpg"],
    },
  }]]);
});

test("an unlocked seed is sent as null and references default to none", async () => {
  let sent;
  const { flow } = setup({ invoke: async (_command, args) => { sent = args.args; return backendResult; } });

  await flow.run(payload);

  assert.equal(sent.seed, null);
  assert.deepEqual(sent.references, []);
});

test("a successful run adds the final result to the history exactly once", async () => {
  const { flow, state, calls } = setup({ invoke: async () => backendResult });

  const outcome = await flow.run({ ...payload, references: ["C:\\photos\\ref.jpg"] });

  assert.equal(outcome.ok, true);
  assert.equal(state.history.length, 1);
  assert.equal(calls.applied.length, 1);
  assert.equal(state.history[0], calls.applied[0]);
  assert.equal(state.history[0].path, backendResult.path);
  assert.deepEqual(state.history[0].meta.references, ["C:\\photos\\a.jpg", "C:\\photos\\ref.jpg"]);
  assert.equal(state.history[0].meta.mode, "masked-edit");
  assert.equal(state.history[0].meta.seed, 42);
  assert.ok(state.sessionOutputs.has(backendResult.path));
});

test("busy state brackets the run and is released afterwards", async () => {
  let duringRun;
  const { flow, state, calls } = setup({
    invoke: async () => { duringRun = { busy: state.busy, begin: calls.begin, end: calls.end }; return backendResult; },
  });

  await flow.run(payload);

  assert.deepEqual(duringRun, { busy: true, begin: 1, end: 0 });
  assert.equal(state.busy, false);
  assert.equal(calls.end, 1);
});

test("a failed run leaves the history alone and reports the error for a retry", async () => {
  const { flow, state, calls } = setup({ invoke: async () => { throw "The engine is off."; } });

  const outcome = await flow.run(payload);

  assert.deepEqual(outcome, { error: "The engine is off." });
  assert.equal(state.history.length, 0);
  assert.equal(calls.applied.length, 0);
  assert.equal(state.busy, false);
  assert.equal(calls.end, 1);
});

test("a cancelled run is not added to the history and is reported as cancelled", async () => {
  const { flow, state, calls } = setup({ invoke: async () => { throw "Cancelled"; } });

  const outcome = await flow.run(payload);

  assert.deepEqual(outcome, { error: "cancelled" });
  assert.equal(state.history.length, 0);
  assert.deepEqual(calls.notices, [["cancelled", true]]);
});

test("a cancel that arrives while the backend still finishes discards the result", async () => {
  const { flow, state } = setup({
    invoke: async () => { state.generationRun += 1; throw new Error("Cancelled; the engine may still be finishing."); },
  });

  const outcome = await flow.run(payload);

  assert.deepEqual(outcome, { error: "cancelled" });
  assert.equal(state.history.length, 0);
});

test("a second submit while busy is blocked without calling the backend", async () => {
  let release;
  let calls = 0;
  const { flow, state } = setup({
    invoke: () => { calls += 1; return new Promise((resolve) => { release = () => resolve(backendResult); }); },
  });

  const first = flow.run(payload);
  const second = await flow.run(payload);
  assert.deepEqual(second, { error: "busy" });
  assert.equal(calls, 1);

  release();
  assert.equal((await first).ok, true);
  assert.equal(state.history.length, 1);
});

test("a normal generation in progress also blocks a masked edit", async () => {
  let called = false;
  const { flow, state } = setup({ invoke: async () => { called = true; return backendResult; } });
  state.busy = true;

  assert.deepEqual(await flow.run(payload), { error: "busy" });
  assert.equal(called, false);
  assert.equal(state.busy, true, "the other run keeps its busy flag");
});
