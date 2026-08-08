// Internal atomic output boundary. A target owns one immutable authority and
// one live constructed stylesheet; mutable transaction state stays in closure.

import { sequenceIdentityMatches } from "./sequence-identity-matches.js";
import { isCanonicalOutputBindingName } from "./output-bindings.js";

const TARGET_STATE = Symbol.for("@labpics/colors/output-sink/target-state/v2");
const PROTOCOL = "@labpics/colors/output-sink/v2";
const VALIDATION_VALUE = "__labcolors_validation__";
const ALWAYS_OWNS = () => true;
const APPLY = Reflect.apply;
const OWN_KEYS = Reflect.ownKeys;
const CREATE_OBJECT = Object.create;
const DEFINE_PROPERTIES = Object.defineProperties;
const DEFINE_PROPERTY = Object.defineProperty;
const FREEZE = Object.freeze;
const GET_OWN_DESCRIPTOR = Object.getOwnPropertyDescriptor;
const GET_PROTOTYPE_OF = Object.getPrototypeOf;
const IS_FROZEN = Object.isFrozen;

function inheritedDescriptor(value, property) {
  let prototype = GET_PROTOTYPE_OF(value);
  while (prototype !== null) {
    const descriptor = GET_OWN_DESCRIPTOR(prototype, property);
    if (descriptor !== undefined) return descriptor;
    prototype = GET_PROTOTYPE_OF(prototype);
  }
  return null;
}

function accessor(value, property) {
  const getter = inheritedDescriptor(value, property)?.get;
  if (typeof getter !== "function") throw new TypeError(`missing ${property} getter`);
  return getter;
}

function method(value, property) {
  const callable = inheritedDescriptor(value, property)?.value;
  if (typeof callable !== "function") throw new TypeError(`missing ${property} method`);
  return callable;
}

function accessorPair(value, property) {
  const descriptor = inheritedDescriptor(value, property);
  if (typeof descriptor?.get !== "function" || typeof descriptor?.set !== "function") {
    throw new TypeError(`missing ${property} accessor pair`);
  }
  return { get: descriptor.get, set: descriptor.set };
}

function captureDomOracle(globalObject) {
  try {
    const document = globalObject.document;
    if (document === null || typeof document !== "object") {
      throw new TypeError("globalThis.document is not an object");
    }
    const nodeType = accessor(document, "nodeType");
    const documentElement = accessor(document, "documentElement");
    const documentDefaultView = accessor(document, "defaultView");
    const documentCreateElement = method(document, "createElement");
    const documentAdopted = accessorPair(document, "adoptedStyleSheets");
    if (APPLY(nodeType, document, []) !== 9) {
      throw new TypeError("globalThis.document does not implement native Document");
    }

    const ambientElement = APPLY(documentElement, document, []);
    if (ambientElement === null || typeof ambientElement !== "object") {
      throw new TypeError("document.documentElement is not an object");
    }
    const ownerDocument = accessor(ambientElement, "ownerDocument");
    const isConnected = accessor(ambientElement, "isConnected");
    const getRootNode = method(ambientElement, "getRootNode");
    const elementShadowRoot = accessor(ambientElement, "shadowRoot");
    const elementStyle = accessor(ambientElement, "style");
    const elementAttachShadow = method(ambientElement, "attachShadow");
    const ambientStyle = APPLY(elementStyle, ambientElement, []);
    const styleLength = accessor(ambientStyle, "length");
    const styleItem = method(ambientStyle, "item");

    const sentinel = APPLY(documentCreateElement, document, ["div"]);
    const sentinelRoot = APPLY(elementAttachShadow, sentinel, [{ mode: "open" }]);
    const shadowHost = accessor(sentinelRoot, "host");
    const shadowMode = accessor(sentinelRoot, "mode");
    const shadowAdopted = accessorPair(sentinelRoot, "adoptedStyleSheets");
    const realm = APPLY(documentDefaultView, document, []);

    if (
      APPLY(nodeType, ambientElement, []) !== 1 ||
      APPLY(ownerDocument, ambientElement, []) !== document ||
      APPLY(getRootNode, ambientElement, []) !== document ||
      APPLY(isConnected, ambientElement, []) !== true ||
      APPLY(nodeType, sentinelRoot, []) !== 11 ||
      APPLY(ownerDocument, sentinelRoot, []) !== document ||
      APPLY(elementShadowRoot, sentinel, []) !== sentinelRoot ||
      APPLY(shadowHost, sentinelRoot, []) !== sentinel ||
      APPLY(shadowMode, sentinelRoot, []) !== "open" ||
      realm === null ||
      (typeof realm !== "object" && typeof realm !== "function") ||
      typeof realm.CSSStyleSheet !== "function"
    ) {
      throw new TypeError("ambient DOM interfaces failed the native self-check");
    }

    return Object.freeze({
      oracle: Object.freeze({
        nodeType,
        ownerDocument,
        isConnected,
        getRootNode,
        elementShadowRoot,
        elementStyle,
        styleLength,
        styleItem,
        documentElement,
        documentDefaultView,
        documentAdopted,
        shadowHost,
        shadowMode,
        shadowAdopted,
      }),
      failure: null,
    });
  } catch (cause) {
    const failure = cause instanceof Error
      ? cause
      : new Error("ambient DOM oracle capture threw a non-Error value", { cause });
    return Object.freeze({ oracle: null, failure });
  }
}

const DOM_CAPTURE = captureDomOracle(globalThis);
const DOM_ORACLE = DOM_CAPTURE.oracle;

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

function assertElementBrand(target, context, stale = false) {
  const ErrorType = stale ? OutputTargetStaleError : OutputTargetCapabilityError;
  if (DOM_ORACLE === null) {
    throw new ErrorType(context, "ambient browser DOM authority is unavailable", {
      cause: DOM_CAPTURE.failure,
    });
  }
  let nodeType;
  try {
    nodeType = APPLY(DOM_ORACLE.nodeType, target, []);
  } catch (cause) {
    throw new ErrorType(context, "target must be a native Element", { cause });
  }
  if (nodeType !== 1) throw new ErrorType(context, "target must be a native Element");
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
    const nodeType = APPLY(DOM_ORACLE.nodeType, root, []);
    const getter = nodeType === 9
      ? DOM_ORACLE.documentAdopted.get
      : nodeType === 11
        ? DOM_ORACLE.shadowAdopted.get
        : null;
    if (getter === null) throw new TypeError("output root must be a Document or ShadowRoot");
    value = APPLY(getter, root, []);
  } catch (cause) {
    throw new ErrorType(context, detail, { cause });
  }
  return asArray(value, context, detail, ErrorType);
}

function observeAdoptedWrite(root, candidate, context) {
  let writeCause = null;
  try {
    const nodeType = APPLY(DOM_ORACLE.nodeType, root, []);
    const setter = nodeType === 9
      ? DOM_ORACLE.documentAdopted.set
      : nodeType === 11
        ? DOM_ORACLE.shadowAdopted.set
        : null;
    if (setter === null) throw new TypeError("output root must be a Document or ShadowRoot");
    APPLY(setter, root, [candidate]);
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

  assertElementBrand(target, context, stale);

  let connected;
  let document;
  let treeRoot;
  let shadowRoot;
  let realm;
  let documentRoot;
  try {
    connected = APPLY(DOM_ORACLE.isConnected, target, []);
    document = APPLY(DOM_ORACLE.ownerDocument, target, []);
    treeRoot = APPLY(DOM_ORACLE.getRootNode, target, []);
    shadowRoot = APPLY(DOM_ORACLE.elementShadowRoot, target, []);
    if (APPLY(DOM_ORACLE.nodeType, document, []) !== 9) {
      fail("target must belong to a Document");
    }
    realm = APPLY(DOM_ORACLE.documentDefaultView, document, []);
    documentRoot = APPLY(DOM_ORACLE.documentElement, document, []);
  } catch (cause) {
    if (cause instanceof ErrorType) throw cause;
    fail("target identity inspection failed", { cause });
  }
  if (connected !== true) fail("target is detached from its document");
  if (!document || !realm) {
    fail("target must belong to a live Document realm");
  }
  if (typeof realm.CSSStyleSheet !== "function") {
    fail("target realm lacks constructed CSSStyleSheet support");
  }

  let root;
  let selector;
  if (documentRoot === target && treeRoot === document) {
    root = document;
    selector = ":root";
  } else {
    try {
      if (
        shadowRoot === null ||
        APPLY(DOM_ORACLE.nodeType, shadowRoot, []) !== 11 ||
        APPLY(DOM_ORACLE.shadowMode, shadowRoot, []) !== "open" ||
        APPLY(DOM_ORACLE.shadowHost, shadowRoot, []) !== target ||
        APPLY(DOM_ORACLE.ownerDocument, shadowRoot, []) !== document
      ) {
        fail("target must be Document.documentElement or the host of its own open ShadowRoot");
      }
      root = shadowRoot;
      selector = ":host";
    } catch (cause) {
      if (cause instanceof ErrorType) throw cause;
      fail("target shadow-root identity inspection failed", { cause });
    }
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
    if (!isCanonicalOutputBindingName(binding)) {
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
    style = APPLY(DOM_ORACLE.elementStyle, target, []);
    length = APPLY(DOM_ORACLE.styleLength, style, []);
  } catch (cause) {
    throw new ErrorType(context, "target inline-style readback failed", { cause });
  }
  if (!style || !Number.isSafeInteger(length) || length < 0) {
    throw new ErrorType(context, "target lacks a readable inline style declaration");
  }
  const declared = new Set();
  try {
    for (let index = 0; index < length; index++) {
      const name = APPLY(DOM_ORACLE.styleItem, style, [index]);
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
    descriptor = GET_OWN_DESCRIPTOR(target, TARGET_STATE);
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
    GET_PROTOTYPE_OF(authority) !== null ||
    !IS_FROZEN(authority)
  ) {
    return false;
  }
  const keys = OWN_KEYS(authority);
  if (keys.length !== 2 || !keys.includes("protocol") || !keys.includes("acquire")) return false;
  const protocol = GET_OWN_DESCRIPTOR(authority, "protocol");
  const acquire = GET_OWN_DESCRIPTOR(authority, "acquire");
  return (
    protocol?.value === PROTOCOL &&
    protocol.writable === false &&
    protocol.configurable === false &&
    typeof acquire?.value === "function" &&
    acquire.writable === false &&
    acquire.configurable === false
  );
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
  return { actualText, adopted: copies === 1, sheets: adopted };
}

function descriptorsIdentical(left, right) {
  return (
    left.document === right.document &&
    left.root === right.root &&
    left.realm === right.realm &&
    left.Sheet === right.Sheet &&
    left.selector === right.selector
  );
}

function assertFresh(record, context, expectedText = record.liveText) {
  const current = describeTarget(record.target, context, true);
  if (!descriptorsIdentical(current, record)) {
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

  const matches = sequenceIdentityMatches(baseline, remaining);
  const matchedBaseline = new Set(matches.map(([baselineIndex]) => baselineIndex));
  const baselineByCurrent = new Map(
    matches.map(([baselineIndex, currentIndex]) => [currentIndex, baselineIndex]),
  );
  const unmatchedBaseline = new Map();
  for (let index = 0; index < baseline.length; index++) {
    if (matchedBaseline.has(index)) continue;
    const entry = baseline[index];
    unmatchedBaseline.set(entry, (unmatchedBaseline.get(entry) ?? 0) + 1);
  }

  const additions = new Array(remaining.length).fill(false);
  for (let index = 0; index < remaining.length; index++) {
    if (baselineByCurrent.has(index)) continue;
    const entry = remaining[index];
    const displaced = unmatchedBaseline.get(entry) ?? 0;
    if (displaced > 0) unmatchedBaseline.set(entry, displaced - 1);
    else additions[index] = true;
  }

  const nextBaselineIndices = new Array(remaining.length).fill(null);
  let nextBaselineIndex = null;
  for (let index = remaining.length - 1; index >= 0; index--) {
    if (baselineByCurrent.has(index)) nextBaselineIndex = baselineByCurrent.get(index);
    else nextBaselineIndices[index] = nextBaselineIndex;
  }
  const additionsBefore = new Map();
  const trailing = [];
  for (let index = 0; index < remaining.length; index++) {
    if (!additions[index]) continue;
    const anchorIndex = nextBaselineIndices[index];
    if (anchorIndex === null) trailing.push(remaining[index]);
    else {
      const anchoredAdditions = additionsBefore.get(anchorIndex) ?? [];
      anchoredAdditions.push(remaining[index]);
      additionsBefore.set(anchorIndex, anchoredAdditions);
    }
  }
  const restored = [];
  for (let index = 0; index < baseline.length; index++) {
    for (const addition of additionsBefore.get(index) ?? []) restored.push(addition);
    restored.push(baseline[index]);
  }
  for (const addition of trailing) restored.push(addition);
  return restored;
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
  let candidate;
  try {
    candidate = adoptionWithoutSheet(current, sheet, baseline);
  } catch (cause) {
    return { exact: false, cause };
  }
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

function reconcileRevoke(record, journal, context) {
  let actualText = null;
  let textReadCause = null;
  try {
    actualText = sheetText(journal.sheet, context, OutputTargetStaleError);
  } catch (cause) {
    textReadCause = cause;
  }
  let textWriteCause = null;
  if (actualText !== journal.expectedText) {
    try {
      journal.sheet.replaceSync(journal.expectedText);
    } catch (cause) {
      textWriteCause = cause;
    }
  }
  let textVerifyCause = null;
  let textExact = false;
  try {
    textExact =
      sheetText(journal.sheet, context, OutputTargetStaleError) === journal.expectedText;
  } catch (cause) {
    textVerifyCause = cause;
  }

  let current = null;
  let adoptionReadCause = null;
  try {
    current = readAdopted(
      journal.root,
      context,
      OutputTargetStaleError,
      "revoke recovery root lacks readable adoptedStyleSheets",
    );
  } catch (cause) {
    adoptionReadCause = cause;
  }
  let adoptionExact = false;
  let adoptionCause = null;
  if (current !== null) {
    try {
      const withoutOwned = current.filter((sheet) => sheet !== journal.sheet);
      const candidate = adoptionWithoutSheet(withoutOwned, journal.sheet, journal.baseline);
      if (arraysIdentical(current, candidate)) adoptionExact = true;
      else {
        const observation = observeAdoptedWrite(journal.root, candidate, context);
        adoptionExact = observation.exact;
        adoptionCause = adoptionExact
          ? null
          : aggregate("revoke adoption recovery failure", [
            observation.writeCause,
            observation.readCause,
          ]);
      }
    } catch (cause) {
      adoptionCause = cause;
    }
  }

  if (textExact && adoptionExact) {
    record.journal = null;
    record.phase = "attached";
    return;
  }
  throw new OutputAtomicityViolationError(
    context,
    "pre-revoke stylesheet state remains unresolved",
    {
      cause: aggregate("revoke recovery failure", [
        journal.cause,
        textReadCause,
        textWriteCause,
        textVerifyCause,
        adoptionReadCause,
        adoptionCause,
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
  if (journal.kind === "revoke") {
    reconcileRevoke(record, journal, context);
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

function installAttachment(record, descriptor, bindings, context) {
  if (record.leases.length !== 0 || record.owners.size !== 0) {
    throw new OutputTargetStaleError(context, "unattached target sink retains active owners");
  }
  if (record.phase === "dormant") {
    if (record.liveText !== "") {
      throw new OutputTargetStaleError(context, "dormant output stylesheet is not empty");
    }
    if (sheetText(record.liveSheet, context, OutputTargetStaleError) !== record.liveText) {
      throw new OutputTargetStaleError(
        context,
        "dormant output stylesheet changed outside its sink",
      );
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
  const written =
    observation.exact && observation.writeCause === null && observation.readCause === null;
  let postconditionCause = null;
  if (written) {
    try {
      const currentDescriptor = describeTarget(record.target, context, true);
      if (!descriptorsIdentical(currentDescriptor, descriptor)) {
        throw new OutputTargetStaleError(
          context,
          "target moved during output stylesheet attachment",
        );
      }
      const inlineConflicts = inlineBindingConflicts(record.target, bindings, context, true);
      if (inlineConflicts.length > 0) {
        throw new OutputInlineBindingConflictError(context, inlineConflicts);
      }
    } catch (cause) {
      postconditionCause = cause;
    }
  }
  if (written && postconditionCause === null) {
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
    postconditionCause,
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

function detachRecord(record, context, prior) {
  const journal = {
    kind: "revoke",
    root: record.root,
    sheet: record.liveSheet,
    baseline: prior.sheets,
    expectedText: prior.actualText,
    cause: null,
  };
  record.journal = journal;
  let current;
  try {
    current = readAdopted(
      record.root,
      context,
      OutputTargetStaleError,
      "dormant output root lacks readable adoptedStyleSheets",
    );
  } catch (cause) {
    journal.cause = cause;
    reconcileRevoke(record, journal, context);
    throw new OutputAtomicityViolationError(
      context,
      "last-lease cleanup could not inspect the output root; prior state was restored",
      { cause },
    );
  }
  journal.baseline = current;
  const candidate = adoptionWithoutSheet(current, record.liveSheet, null);
  const observation = arraysIdentical(current, candidate)
    ? { exact: true, writeCause: null, readCause: null }
    : observeAdoptedWrite(record.root, candidate, context);
  if (observation.exact && observation.writeCause === null && observation.readCause === null) {
    record.journal = null;
    record.phase = "dormant";
    return;
  }
  const detachCause = aggregate("last-lease detach failure", [
    observation.writeCause,
    observation.readCause,
    observation.exact ? null : new Error("adoptedStyleSheets did not match detach candidate"),
  ]);
  journal.cause = detachCause;
  reconcileRevoke(record, journal, context);
  throw new OutputAtomicityViolationError(
    context,
    "last-lease cleanup could not detach the output stylesheet; prior state was restored",
    { cause: detachCause },
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
  if (!lease.active || lease.abandoned) return false;
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
    lease.values = nextValues;
    lease.provisional = false;
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
  lease.provisional = false;
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
  lease.abandoned = false;
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
  if (record.leases.length === 1) detachRecord(record, context, before);
  removeLease(record, lease);
  record.liveText = text;
  if (record.stamp < Number.MAX_SAFE_INTEGER) record.stamp++;
  if (record.epoch < Number.MAX_SAFE_INTEGER) record.epoch++;
  return true;
}

function recoverAbandonedLeases(record, context) {
  for (const lease of [...record.leases]) {
    if (lease.active && lease.abandoned) revoke(record, lease, ALWAYS_OWNS, context);
  }
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
    abandon() {
      if (!state.active) return false;
      if (!state.provisional) {
        throw new OutputTargetStaleError(
          context,
          "only an unpublished provisional lease can enter authority recovery",
        );
      }
      if (record.busy) throw new OutputSinkBusyError(context);
      state.abandoned = true;
      return true;
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
    installAttachment(record, descriptor, bindings, context);
  }
  assertFresh(record, context);

  const conflicts = bindings.filter((binding) => record.owners.has(binding));
  if (conflicts.length > 0) throw new OutputBindingConflictError(context, conflicts);
  const inlineConflicts = inlineBindingConflicts(record.target, bindings, context, true);
  if (inlineConflicts.length > 0) {
    throw new OutputInlineBindingConflictError(context, inlineConflicts);
  }
  const state = {
    bindings,
    bindingSet: new Set(bindings),
    values: new Map(),
    provisional: true,
    abandoned: false,
    active: true,
  };
  record.leases.push(state);
  for (const binding of bindings) record.owners.set(binding, state);
  record.epoch++;
  return leaseHandle(record, state, context);
}

function acquireFromAuthority(record, outputBindings, context) {
  return runExclusive(record, context, () => {
    recoverAbandonedLeases(record, context);
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
  const authority = CREATE_OBJECT(null);
  DEFINE_PROPERTIES(authority, {
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
  FREEZE(authority);
  record.authority = authority;
  try {
    DEFINE_PROPERTY(target, TARGET_STATE, {
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
  const prepared = preflightAcquisition(target, outputBindings, context);
  const existing = authorityDescriptor(target, context);
  if (existing) {
    if (!authorityCompatible(existing)) {
      throw new OutputTargetCapabilityError(
        context,
        "target has an incompatible output-sink authority",
      );
    }
    const acquire = GET_OWN_DESCRIPTOR(existing.value, "acquire").value;
    return APPLY(acquire, existing.value, [prepared.bindings, context]);
  }
  const record = installAuthority(target, context);
  return runExclusive(record, context, () =>
    acquirePrepared(record, prepared.descriptor, prepared.bindings, context),
  );
}

export function acquireOutputLease(target, outputBindings, context = "output sink") {
  return acquireOutputLeaseUnchecked(target, outputBindings, context);
}
