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
export function lowerColorSystem(entries) {
  const builder = new ProgramWireBuilderV3();
  let nextId = 1;
  const id = () => nextId++;
  const explicitUses = new Set(entries.map((entry) => entry.node));
  const inputs = [], useOrder = [...explicitUses], postOrder = [];
  const visited = new Set();
  // Iterative traversal also admits deeply nested client graphs without a JS call-stack limit.
  for (const entry of entries) {
    const stack = [{ node: entry.node, finish: false }];
    while (stack.length > 0) {
      const { node, finish } = stack.pop();
      if (finish) { postOrder.push(node); continue; }
      if (visited.has(node)) continue;
      visited.add(node);
      if (node.kind === "input") { postOrder.push(node); continue; }
      if (!explicitUses.has(node)) useOrder.push(node);
      stack.push({ node, finish: true });
      if (node.policy.tone !== undefined) stack.push({ node: node.policy.tone.from, finish: false });
      stack.push({ node: node.against, finish: false });
    }
  }

  const nodes = new Map(), sources = new Map(), constraints = new Map(), targets = new Map();
  const asSurface = (node) => {
    const compiled = nodes.get(node);
    if (compiled.surface === undefined) {
      compiled.surface = id();
      builder.occurrenceSurface(compiled.surface, compiled.occurrence);
    }
    return compiled.surface;
  };
  for (const node of postOrder) {
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
  }
  // Even an all-fixed declaration gets one complete tuple check under V3.
  builder.assignment(useOrder.filter((node) => node.role.adaptive).map((node) => nodes.get(node).target));

  const outputNodes = new Map();
  const outputs = entries.map(({ name, node }) => {
    if (inputs.some((input) => input.name === name)) authoringError(`compile.outputs.${name}`, "output aliases must not shadow input names");
    let compiled = outputNodes.get(node);
    if (compiled === undefined) {
      const use = nodes.get(node), slot = id();
      // Intermediate paints are legitimate values. This path does not request terminal sink authority.
      builder.output(slot, use.paint);
      compiled = { slot, node };
      outputNodes.set(node, compiled);
    }
    return Object.freeze({ name, ...compiled });
  });
  return {
    wire: builder.finish(),
    inputs: Object.freeze(inputs.map(Object.freeze)),
    outputs: Object.freeze(outputs),
    constraints,
    targets,
  };
}
