import assert from "node:assert/strict";
import test from "node:test";
import { createColorInteraction } from "./interaction.js";

const initial = { canvas: "#e6ddd0", blue: "#0067f4", warm: "#b85d35", opacity: .72 };
const deferred = () => {
  let resolve;
  const promise = new Promise((accept) => { resolve = accept; });
  return { promise, resolve };
};

function harness() {
  const frames = new Map(), programs = [], calls = [], outcomes = [], errors = [];
  let nextFrame = 0;
  const h = { frames, programs, calls, outcomes, errors, delay: undefined, reject: false, failSet: false };
  const viewFor = (program) => ({
    current: null,
    set(context) {
      calls.push({ kind: "set", program, context });
      if (h.failSet) throw new Error("host changed");
      this.current = { state: "ready" };
      return { state: "committed", frame: this.current };
    },
    replace(candidate, context) {
      calls.push({ kind: "replace", program: candidate, context });
      if (h.reject) return { state: "rejected", proposed: { state: "conflict" } };
      const view = viewFor(candidate);
      view.current = { state: "ready" };
      return { state: "committed", frame: view.current, view };
    },
    dispose() { calls.push({ kind: "dispose-view", program }); },
  });
  h.controller = createColorInteraction({
    async compile(state) {
      const program = { state, disposed: 0, dispose() { this.disposed += 1; } };
      programs.push(program);
      const wait = h.delay;
      h.delay = undefined;
      if (wait) await wait.promise;
      return program;
    },
    attach: (program) => viewFor(program),
    onOutcome: (outcome) => outcomes.push(outcome),
    onError: (error) => errors.push(error),
    requestFrame(callback) { frames.set(++nextFrame, callback); return nextFrame; },
    cancelFrame(id) { frames.delete(id); },
  });
  h.flush = () => {
    const [id, callback] = frames.entries().next().value;
    frames.delete(id);
    return callback();
  };
  return h;
}

test("surface, sources and material share one latest-state flush", async () => {
  const h = harness();
  h.controller.schedule(initial);
  h.controller.schedule({ ...initial, blue: "#1251b3" });
  const latest = { ...initial, blue: "#1251b3", canvas: "#283548", opacity: .9 };
  h.controller.schedule(latest);
  assert.equal(h.frames.size, 1);
  await h.flush();
  assert.equal(h.programs.length, 1);
  assert.deepEqual(h.programs[0].state, latest);
  assert.deepEqual(h.calls[0].context, { canvas: latest.canvas });
  h.controller.schedule({ ...latest, canvas: "#6b7f72" });
  await h.flush();
  assert.equal(h.programs.length, 1);
  assert.equal(h.calls.filter(({ kind }) => kind === "set").length, 2);
  h.controller.dispose();
});

test("a surface change during compilation uses that variant with the latest context", async () => {
  const h = harness(), wait = deferred();
  h.delay = wait;
  h.controller.schedule(initial);
  const running = h.flush();
  h.controller.schedule({ ...initial, canvas: "#283548" });
  assert.equal(h.frames.size, 0);
  wait.resolve();
  await running;
  assert.equal(h.programs.length, 1);
  assert.deepEqual(h.calls[0].context, { canvas: "#283548" });
  assert.equal(h.frames.size, 0);
  h.controller.dispose();
});

test("an obsolete authored variant is disposed before another flush can publish", async () => {
  const h = harness(), wait = deferred();
  h.delay = wait;
  h.controller.schedule(initial);
  const running = h.flush();
  const latest = { ...initial, warm: "#ce7332", opacity: .8 };
  h.controller.schedule(latest);
  wait.resolve();
  await running;
  assert.equal(h.programs[0].disposed, 1);
  assert.equal(h.calls.length, 0);
  assert.equal(h.frames.size, 1);
  await h.flush();
  assert.equal(h.programs.length, 2);
  assert.deepEqual(h.programs[1].state, latest);
  assert.equal(h.outcomes.length, 1);
  h.controller.dispose();
});

test("rejected replacement retains the accepted definition and disposes only the candidate", async () => {
  const h = harness();
  h.controller.schedule(initial);
  await h.flush();
  h.reject = true;
  const next = { ...initial, opacity: .9 };
  h.controller.schedule(next);
  await h.flush();
  assert.equal(h.programs[0].disposed, 0);
  assert.equal(h.programs[1].disposed, 1);
  assert.equal(h.controller.current.state, "ready");
  h.reject = false;
  h.controller.schedule({ ...next, canvas: "#283548" });
  await h.flush();
  assert.equal(h.programs.length, 3);
  assert.equal(h.programs[0].disposed, 1);
  assert.equal(h.programs[2].disposed, 0);
  h.controller.dispose();
});

test("page disposal cancels queued work and releases an in-flight compiled variant", async () => {
  const h = harness(), wait = deferred();
  h.delay = wait;
  h.controller.schedule(initial);
  const running = h.flush();
  h.controller.schedule({ ...initial, opacity: .8 });
  h.controller.dispose();
  wait.resolve();
  await running;
  assert.equal(h.programs[0].disposed, 1);
  assert.equal(h.frames.size, 0);
  assert.equal(h.calls.length, 0);
  h.controller.schedule(initial);
  assert.equal(h.frames.size, 0);
});

test("a failed first installation releases its view and program and permits a fresh attempt", async () => {
  const h = harness();
  h.failSet = true;
  h.controller.schedule(initial);
  await h.flush();
  assert.equal(h.programs[0].disposed, 1);
  assert.equal(h.calls.filter(({ kind }) => kind === "dispose-view").length, 1);
  assert.equal(h.controller.current, null);
  assert.equal(h.errors.length, 1);
  h.failSet = false;
  h.controller.schedule(initial);
  await h.flush();
  assert.equal(h.programs.length, 2);
  assert.equal(h.controller.current.state, "ready");
  h.controller.dispose();
});
