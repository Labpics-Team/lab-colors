import {
  ProgramWireBuilderV3,
  WCAG22_SC143_TEXT_DEFAULT_V1,
  WCAG22_SC143_TEXT_LARGE_SCALE_V1,
  WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1,
  WCAG22_SC1411_GRAPHICAL_OBJECT_V1,
} from "./program-wire/abi-v3.js";
import { authoringError } from "./authoring.js";

const criteria = Object.freeze({
  "wcag22-text": WCAG22_SC143_TEXT_DEFAULT_V1,
  "wcag22-large-text": WCAG22_SC143_TEXT_LARGE_SCALE_V1,
  "wcag22-ui": WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1,
  "wcag22-graphic": WCAG22_SC1411_GRAPHICAL_OBJECT_V1,
});

/** Only topology, identity maps and encoding: numerical work belongs to Program. */
export function lowerColorSystem(entries, options) {
  const builder = new ProgramWireBuilderV3();
  let nextId = 1;
  const id = () => nextId++;
  const inputs = [], reachable = new Set();
  const pending = entries.map((entry) => entry.node);
  // Iterative traversal also admits deeply nested client graphs without a JS call-stack limit.
  while (pending.length > 0) {
    const node = pending.pop();
    if (reachable.has(node)) continue;
    reachable.add(node);
    if (node.kind === "use") {
      pending.push(node.against);
      if (node.policy.tone !== undefined) pending.push(node.policy.tone.from);
    }
  }
  // A use can only refer to already-created handles. Declaration order is therefore
  // topological and keeps graph IDs and preferences independent of exported aliases.
  const declaredOrder = (a, b) => a.ordinal - b.ordinal;
  const declaredNodes = [...reachable].sort(declaredOrder);

  const nodes = new Map(), sources = new Map(), constraints = new Map(), targets = new Map();
  const asSurface = (node) => {
    const compiled = nodes.get(node);
    if (compiled.surface === undefined) {
      compiled.surface = id();
      builder.occurrenceSurface(compiled.surface, compiled.occurrence);
    }
    return compiled.surface;
  };
  for (const node of declaredNodes) {
    if (node.kind === "input") {
      const input = { name: node.id, port: id(), surface: id() };
      inputs.push(input);
      nodes.set(node, input);
      builder.surfaceInputPort(input.port).inputSurface(input.surface, input.port);
      continue;
    }
    const target = id(), basePaint = id(), occurrence = id();
    const { role, policy } = node;
    if (role.adaptive) {
      builder.colorTarget(target, role.anchor, {
        ...role.options,
        appearance: occurrence,
        ...(policy.tone === undefined ? {} : {
          tone: { from: nodes.get(policy.tone.from).port, curve: policy.tone.curve },
        }),
      });
    } else {
      let source = sources.get(role);
      if (source === undefined) {
        source = id();
        sources.set(role, source);
        builder.source(source, role.anchor);
      }
      builder.fixedTarget(target, source);
    }
    builder.solidPaint(basePaint, target);
    let paint = basePaint;
    if (policy.opacity !== 1) {
      const opacity = id();
      paint = id();
      builder.opacityInput(opacity, policy.opacity).opacityPaint(paint, basePaint, opacity);
    }
    builder.pointOccurrence(occurrence, paint, asSurface(node.against));
    const compiled = { target, paint, occurrence, node };
    nodes.set(node, compiled);
    targets.set(target, node);
    if (policy.contrast !== undefined) {
      const constraint = id();
      builder.wcag22VisibleUnary(true, constraint, occurrence, criteria[policy.contrast]);
      constraints.set(constraint, { node, requirement: "contrast" });
    }
    if (policy.separation !== undefined) {
      const constraint = id();
      builder.visibleSeparationHard(constraint, occurrence, ...policy.separation);
      constraints.set(constraint, { node, requirement: "separation" });
    }
    if (options.cleanliness !== undefined) {
      // Each declared use is an observable point on its actual backdrop. The
      // appearance may equal its backdrop; conformance does not require a
      // causal change to that pixel or terminal materialization authority.
      const constraint = id();
      builder.nominalSrgb8Visible(true, constraint, occurrence);
      constraints.set(constraint, { node, requirement: "cleanliness" });
    }
  }
  // Even an all-fixed declaration gets one complete tuple check under V3.
  builder.assignment(declaredNodes.filter((node) => node.kind === "use" && node.role.adaptive)
    .map((node) => nodes.get(node).target));

  const outputNodes = new Map();
  for (const node of [...new Set(entries.map((entry) => entry.node))].sort(declaredOrder)) {
    const use = nodes.get(node), slot = id();
    // Intermediate paints are legitimate values. This path does not request terminal sink authority.
    builder.output(slot, use.paint);
    outputNodes.set(node, { slot, node });
  }
  const outputs = entries.map(({ name, node }) => {
    if (inputs.some((input) => input.name === name)) authoringError(`compile.outputs.${name}`, "output aliases must not shadow input names");
    return Object.freeze({ name, ...outputNodes.get(node) });
  });
  return {
    wire: builder.finish(),
    inputs: Object.freeze(inputs.map(Object.freeze)),
    outputs: Object.freeze(outputs),
    constraints,
    targets,
  };
}
