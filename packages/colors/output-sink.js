// Internal atomic output boundary. A target owns one immutable authority and
// one live constructed stylesheet; mutable transaction state stays in closure.

const TARGET_STATE = Symbol.for("@labpics/colors/output-sink/target-state/v2");
const ACQUISITION_STATE = Symbol.for("@labpics/colors/output-sink/acquisition-state/v2");
const PROTOCOL = "@labpics/colors/output-sink/v2";
const VALID_BINDING = /^--[a-z0-9-]+$/u;
const VALIDATION_VALUE = "__labcolors_validation__";
const ALWAYS_OWNS = () => true;

function contextLabel(context) {
  return typeof context === "string" && context.length > 0 ? context : "output sink";
}

export class OutputSinkError extends Error {
  constructor(name, code, context, detail, options) {
    super(`${contextLabel(context)}: ${detail}`, options);
    this.name = name;
    this.code = code;
  }
}

export class OutputBindingError extends OutputSinkError {
  constructor(code, context, detail, options) {
    super("OutputBindingError", code, context, detail, options);
  }
}

export class OutputInlineBindingConflictError extends OutputSinkError {
  constructor(context, bindings) {
    const stable = Object.freeze([...bindings].sort(compareStrings));
    super(
      "OutputInlineBindingConflictError",
      "OUTPUT_INLINE_BINDING_CONFLICT",
      context,
      `inline style already declares owned outputs: ${stable.join(", ")}`,
    );
    this.bindings = stable;
  }
}

export class OutputBindingConflictError extends OutputSinkError {
  constructor(context, bindings) {
    const stable = Object.freeze([...bindings].sort(compareStrings));
    super(
      "OutputBindingConflictError",
      "OUTPUT_BINDING_CONFLICT",
      context,
      `output bindings already have an active owner: ${stable.join(", ")}`,
    );
    this.bindings = stable;
  }
}

export class OutputTargetCapabilityError extends OutputSinkError {
  constructor(context, detail, options) {
    super("OutputTargetCapabilityError", "OUTPUT_TARGET_CAPABILITY", context, detail, options);
  }
}

export class OutputTargetStaleError extends OutputSinkError {
  constructor(context, detail, options) {
    super("OutputTargetStaleError", "OUTPUT_TARGET_STALE", context, detail, options);
  }
}

export class OutputStylesheetValidationError extends OutputSinkError {
  constructor(context, detail, options) {
    super(
      "OutputStylesheetValidationError",
      "OUTPUT_STYLESHEET_INVALID",
      context,
      detail,
      options,
    );
  }
}

export class OutputSinkBusyError extends OutputSinkError {
  constructor(context) {
    super(
      "OutputSinkBusyError",
      "OUTPUT_SINK_BUSY",
      context,
      "the target sink is already preparing or committing a publication",
    );
  }
}

export class OutputAtomicityViolationError extends OutputSinkError {
  constructor(context, detail, options) {
    super(
      "OutputAtomicityViolationError",
      "OUTPUT_ATOMICITY_VIOLATION",
      context,
      detail,
      options,
    );
  }
}

function compareStrings(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}

function aggregate(label, causes) {
  const present = causes.filter((cause) => cause !== null && cause !== undefined);
  return present.length === 1 ? present[0] : new AggregateError(present, label);
}

function arraysIdentical(left, right) {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function asArray(value, context, detail, ErrorType = OutputTargetCapabilityError) {
  try {
    return Array.from(value);
  } catch (cause) {
    throw new ErrorType(context, detail, { cause });
  }
}

function readAdopted(root, context, ErrorType, detail) {
  let value;
  try {
    value = root.adoptedStyleSheets;
  } catch (cause) {
    throw new ErrorType(context, detail, { cause });
  }
  return asArray(value, context, detail, ErrorType);
}

function observeAdoptedWrite(root, candidate, context) {
  let writeCause = null;
  try {
    root.adoptedStyleSheets = candidate;
  } catch (cause) {
    writeCause = cause;
  }

  let actual = null;
  let readCause = null;
  try {
    actual = readAdopted(
      root,
      context,
      OutputTargetCapabilityError,
      "target root rejected adoptedStyleSheets readback",
    );
  } catch (cause) {
    readCause = cause;
  }
  return {
    exact: actual !== null && arraysIdentical(actual, candidate),
    writeCause,
    readCause,
  };
}

function describeTarget(target, context, stale = false) {
  const ErrorType = stale ? OutputTargetStaleError : OutputTargetCapabilityError;
  const fail = (detail, options) => {
    throw new ErrorType(context, detail, options);
  };

  if (target === null || typeof target !== "object" || target.nodeType !== 1) {
    fail("target must be an element-like node");
  }
  if (typeof target.getRootNode !== "function") {
    fail("target lacks structural root inspection");
  }

  let connected;
  let document;
  let treeRoot;
  let shadowRoot;
  let realm;
  try {
    connected = target.isConnected;
    document = target.ownerDocument;
    treeRoot = target.getRootNode();
    shadowRoot = target.shadowRoot;
    realm = document?.defaultView;
  } catch (cause) {
    fail("target identity inspection failed", { cause });
  }
  if (connected !== true) fail("target is detached from its document");
  if (!document || document.nodeType !== 9 || !realm) {
    fail("target must belong to a live Document realm");
  }
  if (typeof realm.CSSStyleSheet !== "function") {
    fail("target realm lacks constructed CSSStyleSheet support");
  }

  let root;
  let selector;
  if (document.documentElement === target && treeRoot === document) {
    root = document;
    selector = ":root";
  } else if (
    shadowRoot?.nodeType === 11 &&
    shadowRoot.mode === "open" &&
    shadowRoot.host === target &&
    shadowRoot.ownerDocument === document
  ) {
    root = shadowRoot;
    selector = ":host";
  } else {
    fail("target must be Document.documentElement or the host of its own open ShadowRoot");
  }

  readAdopted(
    root,
    context,
    ErrorType,
    "target output root lacks readable adoptedStyleSheets",
  );
  return { target, document, root, realm, Sheet: realm.CSSStyleSheet, selector };
}

function materializeBindings(bindings, context) {
  if (!Array.isArray(bindings)) {
    throw new OutputBindingError(
      "OUTPUT_BINDINGS_INVALID",
      context,
      "outputBindings must be an array",
    );
  }
  const result = [];
  const seen = new Set();
  for (let index = 0; index < bindings.length; index++) {
    const descriptor = Object.getOwnPropertyDescriptor(bindings, String(index));
    if (!descriptor || !("value" in descriptor) || typeof descriptor.value !== "string") {
      throw new OutputBindingError(
        "OUTPUT_BINDINGS_INVALID",
        context,
        `outputBindings[${index}] must be a string data property`,
      );
    }
    const binding = descriptor.value;
    if (!VALID_BINDING.test(binding)) {
      throw new OutputBindingError(
        "OUTPUT_BINDING_INVALID",
        context,
        `output binding '${binding}' is not a canonical CSS custom property name`,
      );
    }
    if (seen.has(binding)) {
      throw new OutputBindingError(
        "OUTPUT_BINDINGS_INVALID",
        context,
        `outputBindings contains duplicate '${binding}'`,
      );
    }
    seen.add(binding);
    result.push(binding);
  }
  return Object.freeze(result);
}

function inlineBindingConflicts(target, bindings, context, stale = false) {
  const ErrorType = stale ? OutputTargetStaleError : OutputTargetCapabilityError;
  let style;
  let length;
  try {
    style = target.style;
    length = style?.length;
  } catch (cause) {
    throw new ErrorType(context, "target inline-style readback failed", { cause });
  }
  if (!style || typeof style.item !== "function" || !Number.isSafeInteger(length) || length < 0) {
    throw new ErrorType(context, "target lacks a readable inline style declaration");
  }
  const declared = new Set();
  try {
    for (let index = 0; index < length; index++) {
      const name = style.item(index);
      if (typeof name !== "string") {
        throw new TypeError("style.item() returned a non-string property name");
      }
      declared.add(name);
    }
  } catch (cause) {
    throw new ErrorType(context, "target inline-style enumeration failed", { cause });
  }
  return bindings.filter((binding) => declared.has(binding));
}

function preflightAcquisition(target, outputBindings, context) {
  const descriptor = describeTarget(target, context);
  const bindings = materializeBindings(outputBindings, context);
  const inlineConflicts = inlineBindingConflicts(target, bindings, context);
  if (inlineConflicts.length > 0) {
    throw new OutputInlineBindingConflictError(context, inlineConflicts);
  }
  return { descriptor, bindings };
}

function newSheet(Sheet, context) {
  let sheet;
  try {
    sheet = new Sheet();
  } catch (cause) {
    throw new OutputTargetCapabilityError(
      context,
      "target realm could not construct a CSSStyleSheet",
      { cause },
    );
  }
  if (typeof sheet?.replaceSync !== "function") {
    throw new OutputTargetCapabilityError(
      context,
      "constructed CSSStyleSheet lacks replaceSync",
    );
  }
  return sheet;
}

function sheetText(sheet, context, ErrorType = OutputTargetCapabilityError) {
  try {
    return Array.from(sheet.cssRules, (rule) => rule.cssText).join("\n");
  } catch (cause) {
    throw new ErrorType(context, "live CSSStyleSheet readback failed", { cause });
  }
}

function oneRule(sheet, selector, context) {
  let rules;
  try {
    rules = Array.from(sheet.cssRules);
  } catch (cause) {
    throw new OutputStylesheetValidationError(context, "scratch CSSOM readback failed", {
      cause,
    });
  }
  if (rules.length !== 1 || rules[0]?.selectorText !== selector || !rules[0]?.style) {
    throw new OutputStylesheetValidationError(
      context,
      "scratch CSSOM did not preserve the exact target rule",
    );
  }
  return rules[0];
}

function prepareText(record, values, context) {
  if (values.size === 0) return "";
  try {
    record.scratchSheet.replaceSync(`${record.selector} {}`);
  } catch (cause) {
    throw new OutputStylesheetValidationError(
      context,
      "scratch CSSOM rejected the target rule",
      { cause },
    );
  }
  const rule = oneRule(record.scratchSheet, record.selector, context);
  for (const [name, value] of values) {
    try {
      rule.style.setProperty(name, value);
    } catch (cause) {
      throw new OutputStylesheetValidationError(
        context,
        `scratch CSSOM rejected value for '${name}'`,
        { cause },
      );
    }
  }

  const readNames = [];
  for (let index = 0; index < rule.style.length; index++) readNames.push(rule.style.item(index));
  const expectedNames = [...values.keys()];
  if (
    readNames.length !== expectedNames.length ||
    readNames.some((name, index) => name !== expectedNames[index])
  ) {
    throw new OutputStylesheetValidationError(
      context,
      "scratch CSSOM declaration order differs from the canonical output order",
    );
  }
  for (const [name, expected] of values) {
    if (rule.style.getPropertyValue(name) !== expected) {
      throw new OutputStylesheetValidationError(
        context,
        `scratch CSSOM did not preserve the exact value for '${name}'`,
      );
    }
  }

  const text = rule.cssText;
  if (
    typeof text !== "string" ||
    text.length === 0 ||
    sheetText(record.scratchSheet, context) !== text
  ) {
    throw new OutputStylesheetValidationError(
      context,
      "scratch CSSOM did not provide an exact complete stylesheet readback",
    );
  }
  return text;
}

function materializeVars(vars, bindingSet, context) {
  if (vars === null || typeof vars !== "object" || Array.isArray(vars)) {
    throw new OutputBindingError("OUTPUT_VALUES_INVALID", context, "vars must be an object");
  }
  const values = new Map();
  for (const key of Reflect.ownKeys(vars)) {
    if (typeof key !== "string") {
      throw new OutputBindingError(
        "OUTPUT_VALUES_INVALID",
        context,
        "vars must not contain symbol properties",
      );
    }
    const descriptor = Object.getOwnPropertyDescriptor(vars, key);
    if (!descriptor?.enumerable || !("value" in descriptor) || typeof descriptor.value !== "string") {
      throw new OutputBindingError(
        "OUTPUT_VALUES_INVALID",
        context,
        `vars['${key}'] must be an enumerable string data property`,
      );
    }
    if (!bindingSet.has(key)) {
      throw new OutputBindingError(
        "OUTPUT_VALUE_OUTSIDE_BINDINGS",
        context,
        `vars contains unowned output '${key}'`,
      );
    }
    if (descriptor.value.length === 0) {
      throw new OutputBindingError(
        "OUTPUT_VALUES_INVALID",
        context,
        `vars['${key}'] must not be empty`,
      );
    }
    values.set(key, descriptor.value);
  }
  return values;
}

function mergedValues(record, changedLease, changedValues, disposedLease = null) {
  const merged = new Map();
  for (const lease of record.leases) {
    if (!lease.active || lease === disposedLease) continue;
    const values = lease === changedLease ? changedValues : lease.values;
    for (const binding of lease.bindings) {
      if (values.has(binding)) merged.set(binding, values.get(binding));
    }
  }
  return merged;
}

function mapsIdentical(left, right) {
  if (left.size !== right.size) return false;
  const leftEntries = [...left];
  const rightEntries = [...right];
  return leftEntries.every(
    ([name, value], index) =>
      name === rightEntries[index][0] && value === rightEntries[index][1],
  );
}

function ownsNow(owns, context) {
  if (typeof owns !== "function") {
    throw new TypeError(`${contextLabel(context)}: owns must be a function`);
  }
  return owns() === true;
}

function authorityDescriptor(target, context) {
  let descriptor;
  try {
    descriptor = Object.getOwnPropertyDescriptor(target, TARGET_STATE);
  } catch (cause) {
    throw new OutputTargetCapabilityError(context, "target authority inspection failed", { cause });
  }
  return descriptor;
}

function authorityCompatible(descriptor) {
  if (!descriptor || !("value" in descriptor)) return false;
  const authority = descriptor.value;
  if (
    descriptor.configurable !== false ||
    descriptor.writable !== false ||
    authority === null ||
    typeof authority !== "object" ||
    Object.getPrototypeOf(authority) !== null ||
    !Object.isFrozen(authority)
  ) {
    return false;
  }
  const keys = Reflect.ownKeys(authority);
  if (keys.length !== 2 || !keys.includes("protocol") || !keys.includes("acquire")) return false;
  const protocol = Object.getOwnPropertyDescriptor(authority, "protocol");
  const acquire = Object.getOwnPropertyDescriptor(authority, "acquire");
  return (
    protocol?.value === PROTOCOL &&
    protocol.writable === false &&
    protocol.configurable === false &&
    typeof acquire?.value === "function" &&
    acquire.writable === false &&
    acquire.configurable === false
  );
}

function admissionCompatible(descriptor) {
  if (!descriptor || !("value" in descriptor)) return false;
  const authority = descriptor.value;
  if (
    descriptor.configurable !== false ||
    descriptor.writable !== false ||
    authority === null ||
    typeof authority !== "object" ||
    Object.getPrototypeOf(authority) !== null ||
    !Object.isFrozen(authority)
  ) {
    return false;
  }
  const keys = Reflect.ownKeys(authority);
  if (keys.length !== 2 || !keys.includes("protocol") || !keys.includes("run")) return false;
  const protocol = Object.getOwnPropertyDescriptor(authority, "protocol");
  const run = Object.getOwnPropertyDescriptor(authority, "run");
  return (
    protocol?.value === PROTOCOL &&
    protocol.writable === false &&
    protocol.configurable === false &&
    typeof run?.value === "function" &&
    run.writable === false &&
    run.configurable === false
  );
}

function admissionAuthority(context) {
  let descriptor;
  try {
    descriptor = Object.getOwnPropertyDescriptor(globalThis, ACQUISITION_STATE);
  } catch (cause) {
    throw new OutputTargetCapabilityError(context, "shared acquisition gate inspection failed", {
      cause,
    });
  }
  if (descriptor) {
    if (!admissionCompatible(descriptor)) {
      throw new OutputTargetCapabilityError(
        context,
        "shared acquisition gate has an incompatible authority",
      );
    }
    return descriptor.value;
  }

  const active = new WeakSet();
  const authority = Object.create(null);
  Object.defineProperties(authority, {
    protocol: {
      value: PROTOCOL,
      configurable: false,
      enumerable: false,
      writable: false,
    },
    run: {
      value(target, acquireContext, operation) {
        if (active.has(target)) throw new OutputSinkBusyError(acquireContext);
        active.add(target);
        try {
          return operation();
        } finally {
          active.delete(target);
        }
      },
      configurable: false,
      enumerable: false,
      writable: false,
    },
  });
  Object.freeze(authority);
  try {
    Object.defineProperty(globalThis, ACQUISITION_STATE, {
      value: authority,
      configurable: false,
      enumerable: false,
      writable: false,
    });
  } catch (cause) {
    const raced = Object.getOwnPropertyDescriptor(globalThis, ACQUISITION_STATE);
    if (admissionCompatible(raced)) return raced.value;
    throw new OutputTargetCapabilityError(context, "shared acquisition gate installation failed", {
      cause,
    });
  }
  return authority;
}

function assertAuthority(record, context) {
  const descriptor = authorityDescriptor(record.target, context);
  if (!authorityCompatible(descriptor) || descriptor.value !== record.authority) {
    throw new OutputTargetStaleError(context, "target output authority changed");
  }
}

function assertStoredFresh(record, context, expectedText = record.liveText) {
  if (record.phase !== "attached") {
    throw new OutputTargetStaleError(context, "target sink is not attached");
  }
  assertAuthority(record, context);
  const adopted = readAdopted(
    record.root,
    context,
    OutputTargetStaleError,
    "stored output root lacks readable adoptedStyleSheets",
  );
  if (adopted.filter((sheet) => sheet === record.liveSheet).length !== 1) {
    throw new OutputTargetStaleError(context, "live output stylesheet was detached or duplicated");
  }
  if (sheetText(record.liveSheet, context, OutputTargetStaleError) !== expectedText) {
    throw new OutputTargetStaleError(context, "live output stylesheet changed outside its sink");
  }
}

function revocableSnapshot(record, context, expectedText = null) {
  if (record.phase !== "attached") {
    throw new OutputTargetStaleError(context, "target sink is not attached");
  }
  assertAuthority(record, context);
  const adopted = readAdopted(
    record.root,
    context,
    OutputTargetStaleError,
    "stored output root lacks readable adoptedStyleSheets",
  );
  const copies = adopted.filter((sheet) => sheet === record.liveSheet).length;
  if (copies > 1) {
    throw new OutputTargetStaleError(context, "live output stylesheet was duplicated");
  }
  const actualText = sheetText(record.liveSheet, context, OutputTargetStaleError);
  if (expectedText !== null && actualText !== expectedText) {
    throw new OutputTargetStaleError(context, "live output stylesheet bytes are not exact");
  }
  return { actualText, adopted: copies === 1 };
}

function assertFresh(record, context, expectedText = record.liveText) {
  const current = describeTarget(record.target, context, true);
  if (
    current.document !== record.document ||
    current.root !== record.root ||
    current.realm !== record.realm ||
    current.Sheet !== record.Sheet ||
    current.selector !== record.selector
  ) {
    throw new OutputTargetStaleError(context, "target moved to another output identity or realm");
  }
  assertStoredFresh(record, context, expectedText);
  const inlineConflicts = inlineBindingConflicts(
    record.target,
    [...record.owners.keys()],
    context,
    true,
  );
  if (inlineConflicts.length > 0) {
    throw new OutputInlineBindingConflictError(context, inlineConflicts);
  }
}

function adoptionWithoutSheet(current, sheet, baseline) {
  const remaining = current.filter((entry) => entry !== sheet);
  if (baseline === null) return remaining;

  const unmatchedBaseline = [...baseline];
  const additions = [];
  for (const entry of remaining) {
    const index = unmatchedBaseline.indexOf(entry);
    if (index === -1) additions.push(entry);
    else unmatchedBaseline.splice(index, 1);
  }
  return [...baseline, ...additions];
}

function cleanupAdoption(root, sheet, context, baseline = null) {
  let current;
  try {
    current = readAdopted(
      root,
      context,
      OutputTargetCapabilityError,
      "recovery root lacks readable adoptedStyleSheets",
    );
  } catch (cause) {
    return { exact: false, cause };
  }
  const candidate = adoptionWithoutSheet(current, sheet, baseline);
  if (arraysIdentical(current, candidate)) return { exact: true, cause: null };
  const observation = observeAdoptedWrite(root, candidate, context);
  return {
    exact: observation.exact,
    cause: observation.exact
      ? null
      : aggregate("adopted stylesheet cleanup failure", [
        observation.writeCause,
        observation.readCause,
      ]),
  };
}

function reconcileRestore(record, journal, context) {
  let actual = null;
  let firstReadCause = null;
  try {
    actual = sheetText(journal.sheet, context, OutputTargetStaleError);
  } catch (cause) {
    firstReadCause = cause;
  }
  if (actual === journal.expectedText) {
    record.journal = null;
    return;
  }

  let replaceCause = null;
  try {
    journal.sheet.replaceSync(journal.expectedText);
  } catch (cause) {
    replaceCause = cause;
  }
  let verified = false;
  let verifyCause = null;
  try {
    verified =
      sheetText(journal.sheet, context, OutputTargetStaleError) === journal.expectedText;
  } catch (cause) {
    verifyCause = cause;
  }
  if (verified) {
    record.journal = null;
    return;
  }
  throw new OutputAtomicityViolationError(
    context,
    "prior live stylesheet bytes remain unresolved",
    {
      cause: aggregate("stylesheet recovery failure", [
        journal.cause,
        firstReadCause,
        replaceCause,
        verifyCause,
      ]),
    },
  );
}

function reconcile(record, context) {
  const journal = record.journal;
  if (journal === null) return;
  if (journal.kind === "restore") {
    reconcileRestore(record, journal, context);
    return;
  }
  const cleanup = cleanupAdoption(
    journal.root,
    journal.sheet,
    context,
    journal.baseline,
  );
  if (!cleanup.exact) {
    throw new OutputAtomicityViolationError(
      context,
      "residual output stylesheet adoption remains unresolved",
      { cause: aggregate("adoption recovery failure", [journal.cause, cleanup.cause]) },
    );
  }
  record.journal = null;
  if (journal.after === "dormant") record.phase = "dormant";
}

function installAttachment(record, descriptor, context) {
  if (record.leases.length !== 0 || record.owners.size !== 0) {
    throw new OutputTargetStaleError(context, "unattached target sink retains active owners");
  }
  if (record.phase === "dormant") {
    if (record.liveText !== "") {
      throw new OutputTargetStaleError(context, "dormant output stylesheet is not empty");
    }
    const oldAdopted = readAdopted(
      record.root,
      context,
      OutputTargetStaleError,
      "dormant output root lacks readable adoptedStyleSheets",
    );
    if (oldAdopted.includes(record.liveSheet)) {
      throw new OutputTargetStaleError(
        context,
        "dormant output stylesheet was externally re-adopted",
      );
    }
  }

  const sameHost =
    record.phase === "dormant" &&
    descriptor.document === record.document &&
    descriptor.root === record.root &&
    descriptor.realm === record.realm &&
    descriptor.Sheet === record.Sheet &&
    descriptor.selector === record.selector;
  const pendingMatches =
    record.pending !== null &&
    record.pending.document === descriptor.document &&
    record.pending.root === descriptor.root &&
    record.pending.realm === descriptor.realm &&
    record.pending.Sheet === descriptor.Sheet &&
    record.pending.selector === descriptor.selector;
  const pending = pendingMatches
    ? record.pending
    : {
      ...descriptor,
      liveSheet: sameHost ? record.liveSheet : newSheet(descriptor.Sheet, context),
      scratchSheet: sameHost ? record.scratchSheet : newSheet(descriptor.Sheet, context),
    };
  record.pending = pending;

  const current = readAdopted(
    descriptor.root,
    context,
    OutputTargetCapabilityError,
    "target output root lacks readable adoptedStyleSheets",
  );
  if (current.includes(pending.liveSheet)) {
    throw new OutputTargetStaleError(context, "attachment candidate is already adopted");
  }
  const candidate = [...current, pending.liveSheet];
  record.journal = {
    kind: "adoption",
    root: descriptor.root,
    sheet: pending.liveSheet,
    baseline: current,
    after: null,
    cause: null,
  };
  const observation = observeAdoptedWrite(descriptor.root, candidate, context);
  if (observation.exact && observation.writeCause === null && observation.readCause === null) {
    record.journal = null;
    record.pending = null;
    record.document = descriptor.document;
    record.root = descriptor.root;
    record.realm = descriptor.realm;
    record.Sheet = descriptor.Sheet;
    record.selector = descriptor.selector;
    record.liveSheet = pending.liveSheet;
    record.scratchSheet = pending.scratchSheet;
    record.liveText = "";
    record.phase = "attached";
    return;
  }

  const attachmentCause = aggregate("output stylesheet attachment failure", [
    observation.writeCause,
    observation.readCause,
    observation.exact ? null : new Error("adoptedStyleSheets did not match the exact candidate"),
  ]);
  record.journal.cause = attachmentCause;
  const cleanup = cleanupAdoption(
    descriptor.root,
    pending.liveSheet,
    context,
    current,
  );
  if (cleanup.exact) {
    record.journal = null;
    throw new OutputTargetCapabilityError(context, "atomic sink attachment failed", {
      cause: attachmentCause,
    });
  }
  throw new OutputAtomicityViolationError(
    context,
    "sink attachment failed and its residual adoption could not be removed",
    { cause: aggregate("attachment and rollback failure", [attachmentCause, cleanup.cause]) },
  );
}

function detachRecord(record, context) {
  let current;
  try {
    current = readAdopted(
      record.root,
      context,
      OutputTargetStaleError,
      "dormant output root lacks readable adoptedStyleSheets",
    );
  } catch (cause) {
    record.journal = {
      kind: "adoption",
      root: record.root,
      sheet: record.liveSheet,
      baseline: null,
      after: "dormant",
      cause,
    };
    throw new OutputAtomicityViolationError(
      context,
      "last-lease cleanup could not inspect the dormant output root",
      { cause },
    );
  }
  const baseline = current.filter((sheet) => sheet !== record.liveSheet);
  record.journal = {
    kind: "adoption",
    root: record.root,
    sheet: record.liveSheet,
    baseline,
    after: "dormant",
    cause: null,
  };
  const cleanup = cleanupAdoption(record.root, record.liveSheet, context, baseline);
  if (cleanup.exact) {
    record.journal = null;
    record.phase = "dormant";
    return;
  }
  record.journal.cause = cleanup.cause;
  throw new OutputAtomicityViolationError(
    context,
    "last-lease cleanup could not detach the dormant output stylesheet",
    { cause: cleanup.cause },
  );
}

function restorePriorBytes(record, priorText, context, cause) {
  record.journal = {
    kind: "restore",
    sheet: record.liveSheet,
    expectedText: priorText,
    cause,
  };
  reconcileRestore(record, record.journal, context);
}

function replaceCandidate(
  record,
  text,
  owns,
  context,
  postcondition,
  priorText = record.liveText,
) {
  record.journal = {
    kind: "restore",
    sheet: record.liveSheet,
    expectedText: priorText,
    cause: null,
  };

  let replaceCause = null;
  try {
    record.liveSheet.replaceSync(text);
  } catch (cause) {
    replaceCause = cause;
  }
  let actual = null;
  let readCause = null;
  try {
    actual = sheetText(record.liveSheet, context, OutputTargetStaleError);
  } catch (cause) {
    readCause = cause;
  }

  if (replaceCause !== null && readCause === null && actual === priorText) {
    record.journal = null;
    throw replaceCause;
  }
  if (replaceCause !== null || readCause !== null || actual !== text) {
    const failure = aggregate("live stylesheet replacement failure", [
      replaceCause,
      readCause,
      actual === text ? null : new Error("live stylesheet bytes did not match the candidate"),
    ]);
    restorePriorBytes(record, priorText, context, failure);
    throw new OutputAtomicityViolationError(
      context,
      "live stylesheet replacement required rollback",
      { cause: failure },
    );
  }

  try {
    postcondition(text);
  } catch (cause) {
    restorePriorBytes(record, priorText, context, cause);
    throw new OutputAtomicityViolationError(
      context,
      "target host state changed during live stylesheet replacement",
      { cause },
    );
  }

  let retained;
  try {
    retained = ownsNow(owns, context);
  } catch (cause) {
    restorePriorBytes(record, priorText, context, cause);
    throw cause;
  }
  if (!retained) {
    restorePriorBytes(record, priorText, context, new Error("output ownership was cancelled"));
    return false;
  }
  try {
    postcondition(text);
  } catch (cause) {
    restorePriorBytes(record, priorText, context, cause);
    throw new OutputAtomicityViolationError(
      context,
      "target host state changed during the final ownership checkpoint",
      { cause },
    );
  }

  record.journal = null;
  return true;
}

function publish(record, lease, rawVars, owns, context) {
  if (!lease.active) return false;
  if (typeof owns !== "function") {
    throw new TypeError(`${contextLabel(context)}: owns must be a function`);
  }
  if (!ownsNow(owns, context)) return false;
  if (record.phase !== "attached") {
    throw new OutputTargetStaleError(context, "target sink is dormant");
  }
  const epoch = record.epoch;
  const nextValues = materializeVars(rawVars, lease.bindingSet, context);
  const merged = mergedValues(record, lease, nextValues);
  const current = mergedValues(record, null, null);

  if (mapsIdentical(merged, current)) {
    assertFresh(record, context);
    if (!ownsNow(owns, context) || !lease.active || record.epoch !== epoch) return false;
    assertFresh(record, context);
    return true;
  }

  const text = prepareText(record, merged, context);
  assertFresh(record, context);
  if (!ownsNow(owns, context) || !lease.active || record.epoch !== epoch) return false;
  assertFresh(record, context);
  if (record.stamp === Number.MAX_SAFE_INTEGER || record.epoch === Number.MAX_SAFE_INTEGER) {
    throw new OutputTargetStaleError(context, "target sink generation space is exhausted");
  }

  const committed = replaceCandidate(
    record,
    text,
    owns,
    context,
    (expected) => assertFresh(record, context, expected),
  );
  if (!committed) return false;
  if (!lease.active || record.epoch !== epoch) {
    restorePriorBytes(
      record,
      record.liveText,
      context,
      new Error("lease generation changed during publication"),
    );
    return false;
  }
  lease.values = nextValues;
  record.liveText = text;
  record.stamp++;
  record.epoch++;
  return true;
}

function removeLease(record, lease) {
  for (const binding of lease.bindings) record.owners.delete(binding);
  const index = record.leases.indexOf(lease);
  if (index !== -1) record.leases.splice(index, 1);
  lease.values = new Map();
  lease.active = false;
}

function revoke(record, lease, owns, context) {
  if (!lease.active) return false;
  if (typeof owns !== "function") {
    throw new TypeError(`${contextLabel(context)}: owns must be a function`);
  }
  if (!ownsNow(owns, context)) return false;
  if (record.phase !== "attached") {
    throw new OutputTargetStaleError(context, "target sink is dormant");
  }
  const epoch = record.epoch;
  const merged = mergedValues(record, null, null, lease);
  const current = mergedValues(record, null, null);
  const text = mapsIdentical(merged, current)
    ? record.liveText
    : prepareText(record, merged, context);
  revocableSnapshot(record, context);
  if (!ownsNow(owns, context) || !lease.active || record.epoch !== epoch) return false;
  const before = revocableSnapshot(record, context);
  if (before.actualText !== text) {
    const committed = replaceCandidate(
      record,
      text,
      owns,
      context,
      (expected) => revocableSnapshot(record, context, expected),
      before.actualText,
    );
    if (!committed) return false;
  }

  if (!lease.active || record.epoch !== epoch) {
    if (text !== record.liveText) {
      restorePriorBytes(
        record,
        record.liveText,
        context,
        new Error("lease generation changed during revocation"),
      );
    }
    return false;
  }
  removeLease(record, lease);
  record.liveText = text;
  if (record.stamp < Number.MAX_SAFE_INTEGER) record.stamp++;
  if (record.epoch < Number.MAX_SAFE_INTEGER) record.epoch++;
  if (record.leases.length === 0) detachRecord(record, context);
  return true;
}

function runExclusive(record, context, operation) {
  if (record.busy) throw new OutputSinkBusyError(context);
  record.busy = true;
  try {
    reconcile(record, context);
    return operation();
  } finally {
    record.busy = false;
  }
}

function leaseHandle(record, state, context) {
  return Object.freeze({
    outputBindings: state.bindings,
    publish(vars, owns = ALWAYS_OWNS) {
      if (!state.active) return false;
      return runExclusive(record, context, () => publish(record, state, vars, owns, context));
    },
    dispose(owns = ALWAYS_OWNS) {
      if (!state.active) return false;
      return runExclusive(record, context, () => revoke(record, state, owns, context));
    },
    get stamp() {
      return record.stamp;
    },
    get state() {
      return state.active ? "active" : "disposed";
    },
  });
}

function acquirePrepared(record, descriptor, bindings, context) {
  if (record.epoch === Number.MAX_SAFE_INTEGER) {
    throw new OutputTargetStaleError(context, "target sink generation space is exhausted");
  }
  if (record.phase === "attached") {
    assertFresh(record, context);
  } else {
    installAttachment(record, descriptor, context);
  }

  const conflicts = bindings.filter((binding) => record.owners.has(binding));
  if (conflicts.length > 0) throw new OutputBindingConflictError(context, conflicts);
  const state = {
    bindings,
    bindingSet: new Set(bindings),
    values: new Map(),
    active: true,
  };
  record.leases.push(state);
  for (const binding of bindings) record.owners.set(binding, state);
  record.epoch++;
  return leaseHandle(record, state, context);
}

function acquireFromAuthority(record, outputBindings, context) {
  return runExclusive(record, context, () => {
    const { descriptor, bindings } = preflightAcquisition(
      record.target,
      outputBindings,
      context,
    );
    return acquirePrepared(record, descriptor, bindings, context);
  });
}

function installAuthority(target, context) {
  const record = {
    target,
    authority: null,
    phase: "new",
    document: null,
    root: null,
    realm: null,
    Sheet: null,
    selector: null,
    liveSheet: null,
    scratchSheet: null,
    liveText: "",
    leases: [],
    owners: new Map(),
    stamp: 0,
    epoch: 0,
    busy: false,
    journal: null,
    pending: null,
  };
  const authority = Object.create(null);
  Object.defineProperties(authority, {
    protocol: {
      value: PROTOCOL,
      configurable: false,
      enumerable: false,
      writable: false,
    },
    acquire: {
      value: (outputBindings, acquireContext) =>
        acquireFromAuthority(record, outputBindings, acquireContext),
      configurable: false,
      enumerable: false,
      writable: false,
    },
  });
  Object.freeze(authority);
  record.authority = authority;
  try {
    Object.defineProperty(target, TARGET_STATE, {
      value: authority,
      configurable: false,
      enumerable: false,
      writable: false,
    });
  } catch (cause) {
    throw new OutputTargetCapabilityError(
      context,
      "target cannot host the immutable output-sink authority",
      { cause },
    );
  }
  return record;
}

/**
 * Acquire a linear owner for an exact custom-property output set.
 *
 * The only target identities are Document.documentElement (`:root`) and the
 * host of its own open ShadowRoot (`:host`). The target retains an immutable
 * authority so failed host effects remain recoverable across module copies.
 */
function acquireOutputLeaseUnchecked(target, outputBindings, context) {
  if (target !== null && (typeof target === "object" || typeof target === "function")) {
    const existing = authorityDescriptor(target, context);
    if (existing) {
      if (!authorityCompatible(existing)) {
        throw new OutputTargetCapabilityError(
          context,
          "target has an incompatible output-sink authority",
        );
      }
      return existing.value.acquire(outputBindings, context);
    }
  }

  const prepared = preflightAcquisition(target, outputBindings, context);
  const raced = authorityDescriptor(target, context);
  if (raced) {
    if (!authorityCompatible(raced)) {
      throw new OutputTargetCapabilityError(
        context,
        "target has an incompatible output-sink authority",
      );
    }
    return raced.value.acquire(outputBindings, context);
  }
  const record = installAuthority(target, context);
  return runExclusive(record, context, () =>
    acquirePrepared(record, prepared.descriptor, prepared.bindings, context),
  );
}

export function acquireOutputLease(target, outputBindings, context = "output sink") {
  if (target === null || (typeof target !== "object" && typeof target !== "function")) {
    return acquireOutputLeaseUnchecked(target, outputBindings, context);
  }
  const admission = admissionAuthority(context);
  return admission.run(target, context, () =>
    acquireOutputLeaseUnchecked(target, outputBindings, context),
  );
}
