import { authoringError, authoredRgb } from "./authoring.js";

export function parseColorContext(graph, values) {
  if (values === null || typeof values !== "object") authoringError("context", "expected an input map", "color_invalid_context");
  let keys;
  try { keys = Object.keys(values); }
  catch { authoringError("context", "input names could not be read", "color_invalid_context"); }
  const names = new Set(graph.inputs.map((input) => input.name));
  for (const key of keys) {
    if (!names.has(key)) authoringError(`context.${key}`, "unknown input", "color_invalid_context");
  }
  const rgb = new Uint8Array(graph.inputs.length * 3), colors = [], unknown = [];
  for (let index = 0; index < graph.inputs.length; index++) {
    const { name } = graph.inputs[index];
    let value;
    try { value = Object.hasOwn(values, name) ? values[name] : undefined; }
    catch { authoringError(`context.${name}`, "input could not be read", "color_invalid_context"); }
    if (value === undefined || value === null) {
      unknown.push(name);
      continue;
    }
    let channels;
    try { channels = authoredRgb(value, `context.${name}`); }
    catch (error) {
      if (error?.code === "color_invalid_definition") error.code = "color_invalid_context";
      throw error;
    }
    rgb.set(channels, index * 3);
    colors.push(Object.freeze({ name, rgb: channels, css: cssColor(channels, 1) }));
  }
  return { rgb, colors: Object.freeze(colors), unknown: Object.freeze(unknown) };
}

export function cssColor(rgb, opacity) {
  return `rgb(${rgb[0]} ${rgb[1]} ${rgb[2]} / ${opacity})`;
}

export function unknownColorFrame(revision, inputs) {
  return Object.freeze({ state: "unknown", revision,
    diagnostics: Object.freeze(inputs.map((input) => Object.freeze({ kind: "unknown-input", input }))) });
}

export function copyReadyColorFrame(snapshot, graph, context) {
  const optimality = snapshot.optimality;
  if (optimality !== "exact" && optimality !== "unfinished") {
    authoringError("resolution.optimality", "Core did not report selection completion", "color_core_contract");
  }
  const cleanliness = snapshot.cleanlinessAssessment();
  if (cleanliness === null) {
    authoringError("resolution.cleanliness", "Core omitted the current quality assessment", "color_core_contract");
  }
  const selections = snapshot.referenceSelections();
  if (selections === null) {
    authoringError("resolution.reference", "Core omitted the current adaptation evidence", "color_core_contract");
  }
  const reference = Object.freeze(selections.map((selection) => {
    const node = graph.targets.get(selection.targetId);
    if (node === undefined || graph.occurrences.get(selection.occurrenceId) !== node) {
      authoringError("resolution.reference", "Core evidence refers to an undeclared use", "color_core_contract");
    }
    return Object.freeze({ role: node.role.id, use: node.handle, outputs: graph.outputNames.get(node),
      preference: selection.preference, capacity: selection.capacity, cases: selection.cases });
  }));
  const table = snapshot.copyOutputTable();
  const bySlot = new Map();
  for (let i = 0; i < table.length; i += 5) {
    const rgb = Object.freeze([table[i + 1], table[i + 2], table[i + 3]]), opacity = table[i + 4];
    bySlot.set(table[i], Object.freeze({ rgb, opacity, css: cssColor(rgb, opacity) }));
  }
  const pairs = graph.outputs.map(({ name, slot }) => {
    const value = bySlot.get(slot);
    if (value === undefined) authoringError(`resolution.${name}`, "Core omitted a declared output", "color_core_contract");
    return [name, value];
  });
  return Object.freeze({ state: "ready", revision: snapshot.revision, optimality, cleanliness, reference,
    context: Object.freeze(Object.fromEntries(context.colors.map(({ name, rgb }) => [name, rgb]))),
    values: Object.freeze(Object.fromEntries(pairs)) });
}

export function copyColorFrame(snapshot, graph, context) {
  if (snapshot.state === "ready") return copyReadyColorFrame(snapshot, graph, context);
  const revision = snapshot.revision;
  if (snapshot.state === "waiting") return unknownColorFrame(revision, context.unknown);
  if (snapshot.state === "stale") return Object.freeze({ state: "stale", revision,
    diagnostics: Object.freeze([
      ...context.unknown.map((input) => Object.freeze({ kind: "unknown-input", input })),
      Object.freeze({ kind: "stale-context", revision }),
    ]) });
  const diagnostics = copyDiagnostics(snapshot, graph);
  if (snapshot.state === "incomplete") return Object.freeze({ state: "unresolved", reason: "search-limit", revision, diagnostics });
  if (snapshot.state === "unsatisfied" || snapshot.state === "failed") {
    return Object.freeze({ state: "conflict", revision, diagnostics });
  }
  authoringError("resolution.state", "Core returned an unknown lifecycle state", "color_core_contract");
}

function copyDiagnostics(snapshot, graph) {
  const report = snapshot.diagnostics();
  const items = [];
  if (snapshot.state === "incomplete" && report !== null) {
    items.push(Object.freeze({ kind: "search-limit", visited: report.searchVisited, budget: report.searchBudget }));
  }
  for (const violation of report?.witness?.violations ?? []) {
    const constraint = violation.kind === "hard-constraint" || violation.kind === "tonal-hierarchy"
      ? graph.constraints.get(violation.constraintId) : undefined;
    if (violation.kind === "tonal-hierarchy") {
      const stronger = graph.occurrences.get(violation.strongerOccurrenceId);
      const weaker = graph.occurrences.get(violation.weakerOccurrenceId);
      if (constraint?.requirement !== "tonal-hierarchy" || stronger === undefined || weaker === undefined
        || !constraint.nodes.includes(stronger) || !constraint.nodes.includes(weaker)) {
        authoringError("resolution.diagnostics", "Core witness refers to an undeclared relation", "color_core_contract");
      }
      const member = (node) => Object.freeze({ role: node.role.id, use: node.handle,
        outputs: graph.outputNames.get(node) });
      items.push(Object.freeze({ kind: "relation", requirement: "tonal-hierarchy", evidence: "candidate-witness",
        stronger: member(stronger), weaker: member(weaker), caseIndex: violation.caseIndex,
        requiredRatio: violation.requiredRatio, actualRatio: violation.actualRatio,
        requiredGap: violation.requiredGap, actualGap: violation.actualGap,
        strongerPolarity: violation.strongerPolarity, weakerPolarity: violation.weakerPolarity }));
      continue;
    }
    const node = constraint?.node ?? graph.targets.get(violation.targetId);
    const requirement = constraint?.requirement ?? {
      "target-domain": "output-domain", "color-identity": "identity", "max-change": "max-change",
    }[violation.kind];
    if (node === undefined || requirement === undefined) {
      authoringError("resolution.diagnostics", "Core witness refers to an undeclared requirement", "color_core_contract");
    }
    const actual = violation.actual ?? violation.source;
    items.push(Object.freeze({
      kind: "requirement", evidence: "candidate-witness", requirement,
      role: node.role.id, use: node.handle,
      outputs: graph.outputNames.get(node),
      ...(violation.caseIndex === undefined ? {} : { caseIndex: violation.caseIndex }),
      ...(actual === undefined ? {} : { actual: Object.freeze([...actual]) }),
      ...(violation.kind === "max-change" ? { distanceSquared: violation.distanceSquared, maximumDistance: violation.maximumDistance } : {}),
    }));
  }
  return Object.freeze(items);
}
