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
export function lowerColorSystem(entries, options, rasterProposal) {
  const builder = new ProgramWireBuilderV3();
  let nextId = 1;
  const id = () => nextId++;
  const inputs = [], reachable = new Set();
  const reachableRelations = new Set();
  const pending = entries.map((entry) => entry.node);
  // Iterative traversal also admits deeply nested client graphs without a JS call-stack limit.
  while (pending.length > 0) {
    const node = pending.pop();
    if (reachable.has(node)) continue;
    reachable.add(node);
    if (node.kind === "use") {
      pending.push(node.against);
      if (node.policy.tone?.from !== undefined) pending.push(node.policy.tone.from);
      for (const relation of options.relationsByUse?.get(node) ?? []) {
        if (relation.ordinal >= options.beforeOrdinal) break;
        if (reachableRelations.has(relation)) continue;
        reachableRelations.add(relation);
        for (const member of relation.nodes) pending.push(member);
      }
    }
  }
  // A use can only refer to already-created handles. Declaration order is therefore
  // topological and keeps graph IDs and legacy reference order independent of exported aliases.
  const declaredOrder = (a, b) => a.ordinal - b.ordinal;
  const declaredNodes = [...reachable].sort(declaredOrder);

  const nodes = new Map(), sources = new Map(), constraints = new Map(), targets = new Map(), occurrences = new Map();
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
        ...(options.reference === undefined ? { appearance: occurrence } : { referenceAppearance: occurrence }),
        ...(policy.tone === undefined ? {} : {
          tone: {
            from: policy.tone.from === undefined ? undefined : nodes.get(policy.tone.from).port,
            curve: policy.tone.curve,
          },
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
    occurrences.set(occurrence, node);
    const requested = rasterProposal?.paints.get(node.handle);
    if (requested !== undefined) {
      // An opaque occurrence of the same base paint pins the actual source,
      // including when the declared use subsequently applies material opacity.
      let checked = occurrence;
      if (rasterProposal.kind === "try-inverse" && paint !== basePaint) {
        checked = id();
        builder.pointOccurrence(checked, basePaint, asSurface(node.against));
      }
      const constraint = id();
      builder.exactVisibleUnary(true, constraint, checked, [requested >>> 16, (requested >>> 8) & 255, requested & 255]);
      constraints.set(constraint, { node, requirement: "visible-reference" });
    }
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
  for (const relation of [...reachableRelations].sort(declaredOrder)) {
    const constraint = id();
    const members = relation.nodes.map((node) => nodes.get(node).occurrence);
    if (relation.requirement === "tonal-hierarchy") builder.preserveTonalHierarchy(constraint, members);
    else builder.visiblePairSeparationHard(constraint, members[0], members[1], ...relation.bounds);
    constraints.set(constraint, relation);
  }
  const reference = options.reference === undefined ? undefined : inputs.map((input) => {
    const rgb = options.reference.get(input.name);
    if (rgb === undefined) authoringError(`colors.options.reference.${input.name}`, "missing authored surface color");
    return [input.port, rgb];
  });
  // Even an all-fixed declaration gets one complete tuple check under V3.
  const adaptive = declaredNodes.filter((node) => node.kind === "use" && node.role.adaptive)
    .map((node) => nodes.get(node).target);
  if (reference === undefined) builder.compose(adaptive);
  else builder.composeReference(adaptive, reference);

  const outputNodes = new Map();
  for (const node of [...new Set(entries.map((entry) => entry.node))].sort(declaredOrder)) {
    const use = nodes.get(node), slot = id();
    // Intermediate paints are legitimate values. This path does not request terminal sink authority.
    builder.output(slot, use.paint);
    outputNodes.set(node, { slot, node });
  }
  const outputs = entries.map(({ name, node }) => {
    if (inputs.some((input) => input.name === name)) authoringError(`${options.operation}.outputs.${name}`, "output aliases must not shadow input names");
    return Object.freeze({ name, ...outputNodes.get(node) });
  });
  // Compile aliases once; frame projection stays linear in the number of uses.
  const outputNames = new Map(declaredNodes.filter((node) => node.kind === "use").map((node) => [node, []]));
  for (const { name, node } of outputs) outputNames.get(node).push(name);
  for (const names of outputNames.values()) Object.freeze(names);
  return {
    wire: builder.finish(),
    inputs: Object.freeze(inputs.map(Object.freeze)),
    outputs: Object.freeze(outputs),
    constraints,
    targets,
    occurrences,
    outputNames,
  };
}
