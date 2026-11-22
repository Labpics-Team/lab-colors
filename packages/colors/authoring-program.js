import init, { compileProgramWire } from "./index.js";
import { authoringError } from "./authoring.js";
import { lowerColorSystem } from "./authoring-graph.js";
import { copyColorFrame, parseColorContext } from "./authoring-frame.js";
import { attachColorView } from "./authoring-view.js";

const programs = new WeakMap();
const scenario = new Uint32Array([1]);

export function colorProgramSource(program) {
  const source = programs.get(program);
  if (source === undefined) authoringError("program", "expected a compiled ColorProgram", "color_foreign_reference");
  source.ensureLive();
  return source;
}

export async function compileColorSystem(entries) {
  const graph = lowerColorSystem(entries);
  await init();
  const runtime = compileProgramWire(graph.wire, 1);
  let disposed = false, busy = false, revision = 0n;
  const ensureLive = () => {
    if (disposed) authoringError("program", "program is disposed", "color_disposed");
  };
  const source = Object.freeze({ graph, ensureLive,
    createRuntime() { ensureLive(); return compileProgramWire(graph.wire, 1); } });
  const program = Object.freeze({
    inputs: Object.freeze(graph.inputs.map((input) => input.name)),
    outputs: Object.freeze(graph.outputs.map((output) => output.name)),
    resolve(values) {
      ensureLive();
      if (busy) authoringError("program.resolve", "program is already updating", "color_busy");
      busy = true;
      let snapshot;
      try {
        const context = parseColorContext(graph, values);
        revision += 1n;
        snapshot = context.unknown.length > 0
          ? runtime.updateUnknown(revision, 1)
          : runtime.updateObserved(revision, scenario, context.rgb, graph.inputs.length);
        return copyColorFrame(snapshot, graph, context);
      } finally {
        snapshot?.free();
        busy = false;
      }
    },
    attach(element, values) {
      ensureLive();
      return attachColorView(source, element, values);
    },
    dispose() {
      if (busy) authoringError("program.dispose", "program is updating", "color_busy");
      if (disposed) return;
      runtime.free();
      disposed = true;
    },
  });
  programs.set(program, source);
  return program;
}
