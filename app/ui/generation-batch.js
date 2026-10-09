(() => {
  function createBatch(count, startedAt, seedLocked, seed) {
    const duration = count === null
      ? null
      : count * 30 * 60 * 1000 + (count - 1) * 2 * 60 * 1000;
    return {
      count,
      completed: 0,
      requestActive: false,
      deadlineMs: duration === null ? null : startedAt + duration,
      seedLocked,
      seed: seedLocked ? seed : null,
      stopAfterCurrent: false,
    };
  }

  function shouldStartNext(batch, now) {
    if (batch.stopAfterCurrent) return false;
    if (batch.count !== null && batch.completed >= batch.count) return false;
    return batch.deadlineMs === null || now < batch.deadlineMs;
  }

  function remainingMs(batch, now) {
    return batch.deadlineMs === null ? null : Math.max(0, batch.deadlineMs - now);
  }

  function requestSeed(batch) {
    return batch.seedLocked ? batch.seed : null;
  }

  const api = Object.freeze({ createBatch, remainingMs, requestSeed, shouldStartNext });
  if (typeof module === "object" && module.exports) module.exports = api;
  else window.QilBatch = api;
})();
