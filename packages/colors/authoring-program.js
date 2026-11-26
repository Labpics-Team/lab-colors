import init, { compileProgramWire } from "./index.js";
import { authoringError } from "./authoring.js";
import { lowerColorSystem } from "./authoring-graph.js";
import { parseColorContext, resolveColorContext } from "./authoring-frame.js";
import { attachColorView } from "./authoring-view.js";
import { captureRasterReference } from "./authoring-raster.js";

const programs = new WeakMap();

export function colorProgramSource(program) {
  const source = programs.get(program);
  if (source === undefined) authoringError("program", "expected a compiled ColorProgram", "color_foreign_reference");
  source.ensureLive();
  return source;
}

export async function compileColorSystem(entries, options) {
  const graph = lowerColorSystem(entries, options);
  await init();
  // Compilation validates Core once. The first independent owner consumes this session.
  let reserved = compileProgramWire(graph.wire, 1), headless;
  let disposed = false, busy = false, revision = 0n;
  const ensureLive = () => {
    if (disposed) authoringError("program", "program is disposed", "color_disposed");
  };
  const source = Object.freeze({ graph, ensureLive,
    createRuntime() {
      ensureLive();
      if (reserved !== undefined) {
        const runtime = reserved;
        reserved = undefined;
        return runtime;
      }
      return compileProgramWire(graph.wire, 1);
    } });
  const program = Object.freeze({
    inputs: Object.freeze(graph.inputs.map((input) => input.name)),
    outputs: Object.freeze(graph.outputs.map((output) => output.name)),
    resolve(values) {
      ensureLive();
      if (busy) authoringError("program.resolve", "program is already updating", "color_busy");
      busy = true;
      try {
        const context = parseColorContext(graph, values);
        headless ??= source.createRuntime();
        return resolveColorContext(headless, graph, context, ++revision);
      } finally {
        busy = false;
      }
    },
    captureRaster(draw) {
      ensureLive();
      const ordinary = source.createRuntime();
      try {
        return captureRasterReference(draw, options.reference, graph, ordinary, (proposal) => {
          const constrained = lowerColorSystem(entries, options, proposal);
          return { graph: constrained, runtime: compileProgramWire(constrained.wire, 1) };
        });
      } catch (error) {
        ordinary.free();
        throw error;
      }
    },
    attach(element, values, options) {
      ensureLive();
      return attachColorView(source, element, values, options, "attach");
    },
    preview(element, values, options) {
      ensureLive();
      return attachColorView(source, element, values, options, "preview");
    },
    dispose() {
      if (busy) authoringError("program.dispose", "program is updating", "color_busy");
      if (disposed) return;
      reserved?.free();
      headless?.free();
      reserved = undefined;
      headless = undefined;
      disposed = true;
    },
  });
  programs.set(program, source);
  return program;
}
