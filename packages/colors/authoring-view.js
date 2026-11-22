import { authoringError } from "./authoring.js";
import { colorProgramSource } from "./authoring-program.js";
import { copyColorFrame, parseColorContext, unknownColorFrame } from "./authoring-frame.js";
import { createCssScope } from "./authoring-css.js";

const scenario = new Uint32Array([1]);

function reject(proposed) { return Object.freeze({ state: "rejected", proposed }); }

function ensureAvailable(state, operation) {
  if (!state.active) authoringError(operation, "view is disposed or replaced", "color_disposed");
  if (state.busy) authoringError(operation, "view is already updating", "color_busy");
}

function update(state, values, replacement) {
  const context = parseColorContext(state.graph, values);
  const revision = ++state.revision;
  if (context.unknown.length > 0) return reject(unknownColorFrame(revision, context.unknown));
  let snapshot, outcome;
  try {
    state.runtime.updateObservedWith(revision, scenario, context.rgb, state.graph.inputs.length, (candidate) => {
      snapshot = candidate;
      const frame = copyColorFrame(candidate, state.graph, context);
      if (frame.state !== "ready") { outcome = reject(frame); return false; }
      const prepared = state.sink.prepare(context, frame);
      const lastUpdate = Object.freeze({ state: "committed", frame });
      outcome = replacement === undefined ? lastUpdate : Object.freeze({ ...lastUpdate, view: replacement });
      state.pendingFrame = frame;
      state.pendingUpdate = lastUpdate;
      state.sink.install(prepared);
      return true;
    });
    if (outcome.state === "committed") {
      state.current = state.pendingFrame;
      state.lastUpdate = state.pendingUpdate;
    }
    return outcome;
  } finally {
    snapshot?.free();
    state.pendingFrame = null;
    state.pendingUpdate = null;
  }
}

function createView(source, runtime, sink, active) {
  const state = { graph: source.graph, runtime, sink, active, busy: false, revision: 0n,
    current: null, lastUpdate: null, pendingFrame: null, pendingUpdate: null };
  const view = Object.freeze({
    get current() { return state.active ? state.current : null; },
    get lastUpdate() { return state.lastUpdate; },
    set(values) {
      ensureAvailable(state, "view.set");
      state.busy = true;
      try {
        const outcome = update(state, values);
        state.lastUpdate = outcome;
        return outcome;
      } finally { state.busy = false; }
    },
    replace(program, values) {
      ensureAvailable(state, "view.replace");
      state.busy = true;
      let nextRuntime, next, committed = false;
      try {
        const nextSource = colorProgramSource(program);
        nextRuntime = nextSource.createRuntime();
        next = createView(nextSource, nextRuntime, state.sink, false);
        const outcome = update(next.state, values, next.view);
        if (outcome.state === "committed") {
          next.state.active = true;
          state.active = false;
          state.current = null;
          committed = true;
          state.runtime.free();
        } else state.lastUpdate = outcome;
        return outcome;
      } finally {
        if (nextRuntime !== undefined && !committed) nextRuntime.free();
        state.busy = false;
      }
    },
    dispose() {
      if (state.busy) authoringError("view.dispose", "view is updating", "color_busy");
      if (!state.active) return;
      state.busy = true;
      try {
        state.sink.dispose();
        state.runtime.free();
        state.active = false;
        state.current = null;
      } finally { state.busy = false; }
    },
  });
  return { view, state };
}

export function attachColorView(source, element, values, options) {
  const runtime = source.createRuntime();
  let sink, result;
  try {
    sink = createCssScope(element, options);
    result = createView(source, runtime, sink, true);
    if (values !== undefined) result.view.set(values);
    return result.view;
  } catch (error) {
    sink?.dispose();
    runtime.free();
    throw error;
  }
}
