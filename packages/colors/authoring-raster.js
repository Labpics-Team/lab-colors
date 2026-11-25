import { RasterReference, rasterPaintProposal } from "./pkg/labcolors.js";
import { ProgramWireError, PROGRAM_WIRE_TOO_MANY_ENTRIES } from "./program-wire/encoding.js";
import { authoringError } from "./authoring.js";
import { copyColorFrame, parseColorContext, resolveColorContext, updateColorSnapshot } from "./authoring-frame.js";

function readBitmap(draw, frame) {
  const image = draw(frame);
  const prototype = globalThis.ImageData?.prototype;
  const read = (key) => Object.getOwnPropertyDescriptor(prototype, key).get.call(image);
  let width, height, data, colorSpace;
  try {
    width = read("width");
    height = read("height");
    data = read("data");
    colorSpace = read("colorSpace");
  } catch {
    authoringError("raster.draw", "expected ImageData from the renderer", "color_invalid_context");
  }
  const typedArray = Object.getPrototypeOf(Uint8ClampedArray.prototype);
  const tag = Object.getOwnPropertyDescriptor(typedArray, Symbol.toStringTag).get.call(data);
  if (colorSpace !== "srgb" || tag !== "Uint8ClampedArray") {
    authoringError("raster.draw", "expected an sRGB rgba-unorm8 bitmap", "color_unsupported_context");
  }
  if (Object.getOwnPropertyDescriptor(typedArray, "length").get.call(data) !== width * height * 4) {
    authoringError("raster.draw", "the rendered pixel buffer is detached", "color_invalid_context");
  }
  return { width, height, data };
}

function retain({ width, height, data }) {
  return new ImageData(new Uint8ClampedArray(data), width, height, { colorSpace: "srgb" });
}

function project(relation, image, frame, reason) {
  return Object.freeze({ state: "ready", scope: "renderer-bitmap", relation, image, frame,
    ...(reason === undefined ? { basis: "exact-bitmap-identity" } : { reason }) });
}

function paintProposals(expectations, frame) {
  const inverse = new Map(), visible = new Map();
  for (const { use, cases } of frame.reference) {
    const { rgb, opacity, variableBackdrop } = expectations.get(use);
    // The first physical case supplies a seed only. Core checks the whole
    // declared domain and all original hard conditions for either proposal.
    const first = rasterPaintProposal(rgb, cases[0].actualBackdrop, opacity);
    if (first !== undefined) inverse.set(use, first);
    if (variableBackdrop || (first !== undefined && cases.slice(1).every(({ actualBackdrop }) =>
      rasterPaintProposal(rgb, actualBackdrop, opacity) !== undefined))) {
      visible.set(use, (rgb[0] << 16) | (rgb[1] << 8) | rgb[2]);
    }
  }
  return { "try-inverse": inverse, "try-visible": visible };
}

function samePaints(left, right) {
  if (left.size !== right.size) return false;
  for (const [use, rgb] of left) if (right.get(use) !== rgb) return false;
  return true;
}

export function captureRasterReference(draw, authoredReference, graph, runtime, constrained) {
  if (typeof draw !== "function") authoringError("raster.draw", "expected a renderer");
  if (authoredReference === undefined) {
    authoringError("raster.reference", "the program requires an authored reference", "color_invalid_definition");
  }
  let revision = 1n;
  const inputs = Object.fromEntries(graph.inputs.map(({ name }) => [name, authoredReference.get(name)]));
  const original = resolveColorContext(runtime, graph, parseColorContext(graph, inputs), revision);
  if (original.state !== "ready") {
    authoringError("raster.reference", "the authored reference has no ready composition", "color_invalid_context");
  }
  const declarations = new Map([...graph.targets.values()].map((node) => [node.handle, node]));
  const expectations = new Map(original.reference.map(({ use, cases }) => {
    const node = declarations.get(use);
    return [use, { rgb: cases[0].actualVisible, opacity: node.policy.opacity, variableBackdrop: node.against.kind === "use" }];
  }));
  const reference = readBitmap(draw, original);
  const field = RasterReference.capture(reference.width, reference.height, reference.data);
  if (field === undefined) {
    authoringError("raster.reference", "the reference requires its full opaque backdrop", "color_unsupported_context");
  }
  const sessions = new Map();
  let busy = false, disposed = false;
  function proposalSession(kind, paints) {
    const cached = sessions.get(kind);
    if (cached && samePaints(cached.paints, paints)) return cached;
    let next;
    try { next = { paints, ...constrained({ kind, paints }) }; }
    catch (error) {
      if (error instanceof ProgramWireError && error.code === PROGRAM_WIRE_TOO_MANY_ENTRIES) return null;
      throw error;
    }
    cached?.runtime.free();
    sessions.set(kind, next);
    return next;
  }
  return Object.freeze({
    resolve(values) {
      if (disposed) authoringError("raster.resolve", "reference is disposed", "color_disposed");
      if (busy) authoringError("raster.resolve", "raster resolution is in progress", "color_busy");
      busy = true;
      let choice;
      try {
        const current = parseColorContext(graph, values);
        const ordinary = resolveColorContext(runtime, graph, current, ++revision);
        if (ordinary.state !== "ready" && ordinary.state !== "modeled") {
          return Object.freeze({ state: "unavailable", proposed: ordinary });
        }
        const normal = readBitmap(draw, ordinary);
        choice = field.start(normal.width, normal.height, normal.data, expectations.size !== 0);
        let plans, retainedNormal, candidate, image;
        for (let action = choice.action(); ; action = choice.action()) {
          if (action === "preserved") return project(action, retain(normal), ordinary);
          if (action === "restored") return project(action, retain(image), candidate);
          if (action !== "try-inverse" && action !== "try-visible") {
            return project("unavailable", retainedNormal ?? retain(normal), ordinary, action);
          }
          plans ??= paintProposals(expectations, ordinary);
          const paints = plans[action];
          if (paints.size === 0) {
            choice.refused(false);
            continue;
          }
          const session = proposalSession(action, paints);
          if (session === null) {
            choice.proposalLimit();
            continue;
          }
          const proposal = updateColorSnapshot(session.runtime, session.graph, current, revision);
          try {
            if (proposal.state !== "ready") {
              choice.refused(proposal.state === "incomplete");
              continue;
            }
            candidate = copyColorFrame(proposal, session.graph, current);
            // Preserve the ordinary result before the renderer may reuse its storage.
            retainedNormal ??= retain(normal);
            image = readBitmap(draw, candidate);
            choice.inspect(field, image.width, image.height, image.data);
          } finally { proposal.free(); }
        }
      } finally {
        choice?.free();
        busy = false;
      }
    },
    dispose() {
      if (busy) authoringError("raster.dispose", "raster resolution is in progress", "color_busy");
      if (!disposed) {
        for (const session of sessions.values()) session.runtime.free();
        runtime.free();
        field.free();
        disposed = true;
      }
    },
  });
}
