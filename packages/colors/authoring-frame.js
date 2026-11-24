import { authoringError, authoredRgb } from "./authoring.js";

function contextValue(value, path) {
  try {
    if (value === null || typeof value !== "object" || Array.isArray(value)) return authoredRgb(value, path);
    for (const key of Object.keys(value)) {
      if (!["kind", "from", "to"].includes(key)) authoringError(`${path}.${key}`, "unknown field option", "color_invalid_context");
    }
    const own = (key) => Object.hasOwn(value, key) ? value[key] : undefined;
    if (own("kind") !== "affine-srgb8") authoringError(`${path}.kind`, "expected affine-srgb8", "color_invalid_context");
    return Object.freeze({ kind: "affine-srgb8",
      from: authoredRgb(own("from"), `${path}.from`), to: authoredRgb(own("to"), `${path}.to`) });
  } catch (error) {
    if (error?.code === "color_invalid_definition") error.code = "color_invalid_context";
    if (error?.code === "color_invalid_context") throw error;
    authoringError(path, "input could not be read", "color_invalid_context");
  }
}

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
  let field;
  for (let index = 0; index < graph.inputs.length; index++) {
    const { name } = graph.inputs[index];
    let value;
    try { value = Object.hasOwn(values, name) ? values[name] : undefined; }
    catch { authoringError(`context.${name}`, "input could not be read", "color_invalid_context"); }
    if (value === undefined || value === null) {
      unknown.push(name);
      continue;
    }
    const copied = contextValue(value, `context.${name}`);
    if (!Array.isArray(copied)) {
      if (field !== undefined) authoringError(`context.${name}`, "only one affine input is supported", "color_unsupported_context");
      field = Object.freeze({ inputIndex: index, name, descriptor: copied });
    }
    // The varying slot is ignored by Core admission; endpoints travel separately.
    rgb.set(Array.isArray(copied) ? copied : copied.from, index * 3);
    colors.push(Object.freeze({ name, value: copied, css: cssContextValue(copied) }));
  }
  return { rgb, colors: Object.freeze(colors), unknown: Object.freeze(unknown), field };
}

export function cssColor(rgb, opacity) {
  return `rgb(${rgb[0]} ${rgb[1]} ${rgb[2]} / ${opacity})`;
}

/** Serialization of the authored source, never an expansion into support samples. */
export function cssContextValue(value) {
  if (!Array.isArray(value)) {
    return `linear-gradient(90deg in srgb, ${cssColor(value.from, 1)} 0%, ${cssColor(value.to, 1)} 100%)`;
  }
  return cssColor(value, 1);
}

function copiedContext(context) {
  return Object.freeze(Object.fromEntries(context.colors.map(({ name, value }) => [name, value])));
}

function declaredContext(context) {
  return context.field === undefined ? {} : { context: copiedContext(context) };
}

export function unknownColorFrame(revision, inputs, context) {
  return Object.freeze({ state: "unknown", revision,
    ...(context === undefined ? {} : declaredContext(context)),
    diagnostics: Object.freeze(inputs.map((input) => Object.freeze({ kind: "unknown-input", input }))) });
}

function copyFieldAssessment(snapshot, context) {
  if (context.field === undefined) return undefined;
  const report = snapshot.fieldAssessment?.();
  const { inputIndex, descriptor } = context.field;
  if (report === null || report === undefined || report.inputIndex !== inputIndex
    || report.sourceModel !== "opaque-affine-encoded-srgb8-half-up-v1"
    || report.domain !== "whole-field" || report.rendering !== "unknown"
    || !descriptor.from.every((byte, i) => byte === report.from[i])
    || !descriptor.to.every((byte, i) => byte === report.to[i])) {
    authoringError("resolution.field", "Core omitted the current declared source assessment", "color_core_contract");
  }
  return Object.freeze({ input: context.field.name, sourceModel: report.sourceModel,
    domain: report.domain, rendering: report.rendering, regionCount: report.regionCount,
    witnesses: Object.freeze(report.witnesses.map((witness) => Object.freeze({
      coordinate: Object.freeze([...witness.coordinate]), nominal: Object.freeze([...witness.nominal]),
    }))) });
}

function fieldScope(field, model) {
  return field === undefined ? {} : { scope: "nominal-model", field,
    coverage: Object.freeze({ model, rendering: field.rendering, sourceModel: field.sourceModel }) };
}

export function copyReadyColorFrame(snapshot, graph, context, field = copyFieldAssessment(snapshot, context)) {
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
  const components = snapshot.selectionComponents();
  if (components === null) {
    authoringError("resolution.selection", "Core omitted the component selection modes", "color_core_contract");
  }
  const selection = Object.freeze(components.map((component) => Object.freeze({
    mode: component.mode,
    uses: Object.freeze(component.targets.map((target) => {
      const node = graph.targets.get(target);
      if (node === undefined) {
        authoringError("resolution.selection", "Core selection refers to an undeclared use", "color_core_contract");
      }
      return Object.freeze({ role: node.role.id, use: node.handle, outputs: graph.outputNames.get(node) });
    })),
  })));
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
  return Object.freeze({ state: field === undefined ? "ready" : "modeled", revision: snapshot.revision,
    optimality, cleanliness, reference, selection, ...fieldScope(field, "complete"),
    ...(field === undefined ? {} : {
      contextCss: Object.freeze(Object.fromEntries(context.colors.map(({ name, css }) => [name, css]))),
    }),
    context: copiedContext(context),
    values: Object.freeze(Object.fromEntries(pairs)) });
}

export function copyColorFrame(snapshot, graph, context) {
  if (snapshot.state === "ready") return copyReadyColorFrame(snapshot, graph, context);
  const revision = snapshot.revision;
  if (snapshot.state === "waiting") return unknownColorFrame(revision, context.unknown, context);
  if (snapshot.state === "stale") return Object.freeze({ state: "stale", revision,
    ...declaredContext(context),
    diagnostics: Object.freeze([
      ...context.unknown.map((input) => Object.freeze({ kind: "unknown-input", input })),
      Object.freeze({ kind: "stale-context", revision }),
    ]) });
  const field = copyFieldAssessment(snapshot, context);
  const diagnostics = copyDiagnostics(snapshot, graph, field);
  if (snapshot.state === "incomplete") return Object.freeze({ state: "unresolved", reason: "search-limit", revision, diagnostics,
    ...declaredContext(context), ...fieldScope(field, "incomplete") });
  if (snapshot.state === "unsatisfied" || snapshot.state === "failed") {
    return Object.freeze({ state: "conflict", revision, diagnostics,
      ...declaredContext(context), ...fieldScope(field, "violated") });
  }
  authoringError("resolution.state", "Core returned an unknown lifecycle state", "color_core_contract");
}

function copyDiagnostics(snapshot, graph, field) {
  const report = snapshot.diagnostics();
  const items = [];
  if (snapshot.state === "incomplete" && report !== null) {
    items.push(Object.freeze({ kind: "search-limit", visited: report.searchVisited, budget: report.searchBudget }));
  }
  for (const violation of report?.witness?.violations ?? []) {
    let fieldWitness;
    if (field !== undefined && violation.caseIndex !== undefined) {
      const witness = field.witnesses[violation.caseIndex];
      if (witness === undefined) authoringError("resolution.diagnostics", "Core witness refers to an unknown field coordinate", "color_core_contract");
      fieldWitness = Object.freeze({ input: field.input, ...witness });
    }
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
        ...(fieldWitness === undefined ? {} : { fieldWitness }),
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
      ...(fieldWitness === undefined ? {} : { fieldWitness }),
      ...(violation.caseIndex === undefined ? {} : { caseIndex: violation.caseIndex }),
      ...(actual === undefined ? {} : { actual: Object.freeze([...actual]) }),
      ...(violation.kind === "max-change" ? { distanceSquared: violation.distanceSquared, maximumDistance: violation.maximumDistance } : {}),
    }));
  }
  return Object.freeze(items);
}
