// Internal atomic output boundary. A lease owns an explicit, immutable set of
// custom properties; the live CSSStyleSheet is shared only by leases attached
// to the same element.

const TARGET_STATE = Symbol.for("@labpics/colors/output-sink/target-state/v1");
const ROOT_STATE = Symbol.for("@labpics/colors/output-sink/root-state/v1");
const ACQUISITION_STATE = Symbol.for("@labpics/colors/output-sink/acquisition-state/v1");
const PROTOCOL = "@labpics/colors/output-sink/v1";
const MARKER_PREFIX = "data-lab-colors-output-sink-";
const VALIDATION_SELECTOR = `[${MARKER_PREFIX}validation]`;
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

function acquisitionTargets(target, context) {
  const descriptor = Object.getOwnPropertyDescriptor(target, ACQUISITION_STATE);
  if (descriptor) {
    const value = descriptor.value;
    let compatible = "value" in descriptor && value?.protocol === PROTOCOL;
    if (compatible) {
      try {
        WeakSet.prototype.has.call(value.targets, target);
      } catch {
        compatible = false;
      }
    }
    if (!compatible) {
      throw new OutputTargetCapabilityError(
        context,
        "target has an incompatible output-sink acquisition registry",
      );
    }
    return value.targets;
  }

  const targets = new WeakSet();
  const value = Object.freeze({ protocol: PROTOCOL, targets });
  try {
    Object.defineProperty(target, ACQUISITION_STATE, {
      value,
      configurable: false,
      enumerable: false,
      writable: false,
    });
  } catch (cause) {
    throw new OutputTargetCapabilityError(
      context,
      "target cannot host the shared output-sink acquisition registry",
      { cause },
    );
  }
  return targets;
}

function asArray(value, context, detail, stale = false) {
  try {
    return Array.from(value);
  } catch (cause) {
    const ErrorType = stale ? OutputTargetStaleError : OutputTargetCapabilityError;
    throw new ErrorType(context, detail, { cause });
  }
}

function describeTarget(target, context, stale = false) {
  const ErrorType = stale ? OutputTargetStaleError : OutputTargetCapabilityError;
  const fail = (detail, options) => {
    throw new ErrorType(context, detail, options);
  };

  if (target === null || typeof target !== "object" || target.nodeType !== 1) {
    fail("target must be an element-like node");
  }
  if (
    typeof target.getRootNode !== "function" ||
    typeof target.hasAttribute !== "function" ||
    typeof target.setAttribute !== "function" ||
    typeof target.removeAttribute !== "function"
  ) {
    fail("target lacks structural root or attribute capabilities");
  }

  let connected;
  try {
    connected = target.isConnected;
  } catch (cause) {
    throw new OutputTargetStaleError(context, "target connectivity readback failed", { cause });
  }
  if (connected !== true) {
    throw new OutputTargetStaleError(context, "target is detached from its root");
  }

  let document;
  let root;
  let realm;
  try {
    document = target.ownerDocument;
    root = target.getRootNode();
    realm = document?.defaultView;
  } catch (cause) {
    fail("target realm inspection failed", { cause });
  }
  if (!document || document.nodeType !== 9 || !realm) {
    fail("target must belong to a live Document realm");
  }
  const documentRoot = root === document && root?.nodeType === 9;
  const shadowRoot =
    root?.nodeType === 11 && root?.host != null && root?.ownerDocument === document;
  if (!documentRoot && !shadowRoot) {
    fail("target root must be its Document or a ShadowRoot from the same Document");
  }
  if (typeof realm.CSSStyleSheet !== "function") {
    fail("target realm lacks constructed CSSStyleSheet support");
  }
  if (typeof root.querySelectorAll !== "function") {
    fail("target root lacks querySelectorAll");
  }

  let adopted;
  try {
    adopted = asArray(
      root.adoptedStyleSheets,
      context,
      "target root lacks readable adoptedStyleSheets",
      stale,
    );
  } catch (cause) {
    if (cause instanceof OutputSinkError) throw cause;
    fail("target root lacks readable adoptedStyleSheets", { cause });
  }
  return { target, document, root, realm, Sheet: realm.CSSStyleSheet, adopted };
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

function oneRule(sheet, selector, context) {
  let rules;
  try {
    rules = asArray(
      sheet.cssRules,
      context,
      "scratch CSSStyleSheet does not expose readable cssRules",
    );
  } catch (cause) {
    if (cause instanceof OutputSinkError) throw cause;
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

function replaceScratch(sheet, selector, context) {
  try {
    sheet.replaceSync(`${selector} {}`);
  } catch (cause) {
    throw new OutputStylesheetValidationError(
      context,
      "scratch CSSOM rejected the target rule",
      { cause },
    );
  }
  return oneRule(sheet, selector, context);
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
    if (seen.has(descriptor.value)) {
      throw new OutputBindingError(
        "OUTPUT_BINDINGS_INVALID",
        context,
        `outputBindings contains duplicate '${descriptor.value}'`,
      );
    }
    seen.add(descriptor.value);
    result.push(descriptor.value);
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

function validateBindingNames(Sheet, bindings, context) {
  const scratch = newSheet(Sheet, context);
  const rule = replaceScratch(scratch, VALIDATION_SELECTOR, context);
  for (const name of bindings) {
    if (!name.startsWith("--")) {
      throw new OutputBindingError(
        "OUTPUT_BINDING_INVALID",
        context,
        `output binding '${name}' is not a CSS custom property name`,
      );
    }
    const before = rule.style.length;
    try {
      rule.style.setProperty(name, VALIDATION_VALUE);
    } catch (cause) {
      throw new OutputBindingError(
        "OUTPUT_BINDING_INVALID",
        context,
        `output binding '${name}' is not a CSS custom property name`,
        { cause },
      );
    }
    if (
      rule.style.length !== before + 1 ||
      rule.style.getPropertyValue(name).trim() !== VALIDATION_VALUE
    ) {
      throw new OutputBindingError(
        "OUTPUT_BINDING_INVALID",
        context,
        `output binding '${name}' is not a CSS custom property name`,
      );
    }
  }
}

function rootState(root, context) {
  const descriptor = Object.getOwnPropertyDescriptor(root, ROOT_STATE);
  if (descriptor) {
    const value = descriptor.value;
    if (
      !("value" in descriptor) ||
      value?.protocol !== PROTOCOL ||
      !Number.isSafeInteger(value.nextMarkerId) ||
      value.nextMarkerId < 1
    ) {
      throw new OutputTargetCapabilityError(
        context,
        "target root has an incompatible output-sink registry",
      );
    }
    return value;
  }
  const value = { protocol: PROTOCOL, nextMarkerId: 1 };
  try {
    Object.defineProperty(root, ROOT_STATE, {
      value,
      configurable: false,
      enumerable: false,
      writable: false,
    });
  } catch (cause) {
    throw new OutputTargetCapabilityError(
      context,
      "target root cannot host the shared output-sink registry",
      { cause },
    );
  }
  return value;
}

function matchesFor(root, selector, context, stale = false) {
  const ErrorType = stale ? OutputTargetStaleError : OutputTargetCapabilityError;
  try {
    return Array.from(root.querySelectorAll(selector));
  } catch (cause) {
    throw new ErrorType(context, "target marker query failed", { cause });
  }
}

function allocateMarker(descriptor, context) {
  const registry = rootState(descriptor.root, context);
  while (Number.isSafeInteger(registry.nextMarkerId)) {
    const name = `${MARKER_PREFIX}${registry.nextMarkerId}`;
    registry.nextMarkerId++;
    const selector = `[${name}]`;
    if (matchesFor(descriptor.root, selector, context).length !== 0) continue;

    try {
      descriptor.target.setAttribute(name, "");
    } catch (cause) {
      try {
        descriptor.target.removeAttribute(name);
      } catch {}
      throw new OutputTargetCapabilityError(context, "target marker installation failed", {
        cause,
      });
    }
    try {
      const matches = matchesFor(descriptor.root, selector, context);
      if (
        !descriptor.target.hasAttribute(name) ||
        matches.length !== 1 ||
        matches[0] !== descriptor.target
      ) {
        throw new Error("target marker did not round-trip uniquely");
      }
    } catch (cause) {
      try {
        descriptor.target.removeAttribute(name);
      } catch {}
      throw new OutputTargetCapabilityError(
        context,
        "target marker could not be bound uniquely",
        { cause },
      );
    }
    return { name, selector };
  }
  throw new OutputTargetCapabilityError(context, "target marker identity space is exhausted");
}

function sheetText(sheet, context, stale = false) {
  const ErrorType = stale ? OutputTargetStaleError : OutputTargetCapabilityError;
  try {
    return Array.from(sheet.cssRules, (rule) => rule.cssText).join("\n");
  } catch (cause) {
    throw new ErrorType(context, "live CSSStyleSheet readback failed", { cause });
  }
}

function attachRecord(descriptor, context) {
  const liveSheet = newSheet(descriptor.Sheet, context);
  const scratchSheet = newSheet(descriptor.Sheet, context);
  const marker = allocateMarker(descriptor, context);
  const record = {
    protocol: PROTOCOL,
    target: descriptor.target,
    document: descriptor.document,
    root: descriptor.root,
    realm: descriptor.realm,
    Sheet: descriptor.Sheet,
    markerName: marker.name,
    selector: marker.selector,
    liveSheet,
    scratchSheet,
    liveText: "",
    leases: new Set(),
    owners: new Map(),
    stamp: 0,
    epoch: 0,
    preparing: false,
    committing: false,
    poisoned: false,
    attached: true,
  };

  try {
    descriptor.root.adoptedStyleSheets = [...descriptor.adopted, liveSheet];
    const current = Array.from(descriptor.root.adoptedStyleSheets);
    if (current.filter((sheet) => sheet === liveSheet).length !== 1) {
      throw new Error("adoptedStyleSheets did not retain the constructed sheet");
    }
    Object.defineProperty(descriptor.target, TARGET_STATE, {
      value: record,
      configurable: false,
      enumerable: false,
      writable: false,
    });
  } catch (cause) {
    try {
      descriptor.root.adoptedStyleSheets = Array.from(
        descriptor.root.adoptedStyleSheets,
      ).filter((sheet) => sheet !== liveSheet);
    } catch {}
    try {
      descriptor.target.removeAttribute(marker.name);
    } catch {}
    throw new OutputTargetCapabilityError(context, "atomic sink attachment failed", { cause });
  }
  return record;
}

function recordFor(target, descriptor, context) {
  const stateDescriptor = Object.getOwnPropertyDescriptor(target, TARGET_STATE);
  if (!stateDescriptor) return attachRecord(descriptor, context);
  const record = stateDescriptor.value;
  if (!("value" in stateDescriptor) || record?.protocol !== PROTOCOL) {
    throw new OutputTargetCapabilityError(
      context,
      "target has an incompatible output-sink registry",
    );
  }
  if (record.attached) {
    assertFresh(record, context);
  } else {
    resumeRecord(record, descriptor, context);
  }
  return record;
}

function resumeRecord(record, descriptor, context) {
  if (record.poisoned || record.leases.size !== 0 || record.owners.size !== 0) {
    throw new OutputTargetStaleError(context, "dormant target sink has inconsistent state");
  }
  if (sheetText(record.liveSheet, context, true) !== "") {
    throw new OutputTargetStaleError(context, "dormant output stylesheet is not empty");
  }
  const oldAdopted = asArray(
    record.root.adoptedStyleSheets,
    context,
    "dormant root lacks readable adoptedStyleSheets",
    true,
  );
  if (oldAdopted.includes(record.liveSheet)) {
    throw new OutputTargetStaleError(context, "dormant output stylesheet was externally adopted");
  }

  const sameHost =
    descriptor.document === record.document &&
    descriptor.root === record.root &&
    descriptor.realm === record.realm &&
    descriptor.Sheet === record.Sheet;
  const liveSheet = sameHost ? record.liveSheet : newSheet(descriptor.Sheet, context);
  const scratchSheet = sameHost ? record.scratchSheet : newSheet(descriptor.Sheet, context);
  const marker = allocateMarker(descriptor, context);
  try {
    descriptor.root.adoptedStyleSheets = [...descriptor.adopted, liveSheet];
    const current = Array.from(descriptor.root.adoptedStyleSheets);
    if (current.filter((sheet) => sheet === liveSheet).length !== 1) {
      throw new Error("adoptedStyleSheets did not retain the dormant sheet");
    }
    record.document = descriptor.document;
    record.root = descriptor.root;
    record.realm = descriptor.realm;
    record.Sheet = descriptor.Sheet;
    record.liveSheet = liveSheet;
    record.scratchSheet = scratchSheet;
    record.markerName = marker.name;
    record.selector = marker.selector;
    record.liveText = "";
    record.attached = true;
  } catch (cause) {
    try {
      descriptor.root.adoptedStyleSheets = Array.from(
        descriptor.root.adoptedStyleSheets,
      ).filter((sheet) => sheet !== liveSheet);
    } catch {}
    try {
      descriptor.target.removeAttribute(marker.name);
    } catch {}
    throw new OutputTargetCapabilityError(context, "dormant sink reattachment failed", { cause });
  }
}

function assertOperational(record, context) {
  if (record.poisoned) {
    throw new OutputTargetStaleError(context, "target sink was invalidated by host drift");
  }
  if (!record.attached) {
    throw new OutputTargetStaleError(context, "target sink is dormant");
  }
}

function assertFresh(record, context, expectedText = record.liveText) {
  assertOperational(record, context);
  const current = describeTarget(record.target, context, true);
  if (
    current.document !== record.document ||
    current.root !== record.root ||
    current.realm !== record.realm ||
    current.Sheet !== record.Sheet
  ) {
    throw new OutputTargetStaleError(context, "target moved to another root or realm");
  }
  const stateDescriptor = Object.getOwnPropertyDescriptor(record.target, TARGET_STATE);
  if (stateDescriptor?.value !== record) {
    throw new OutputTargetStaleError(context, "target sink ownership marker changed");
  }
  let ownsMarker;
  try {
    ownsMarker = record.target.hasAttribute(record.markerName);
  } catch (cause) {
    throw new OutputTargetStaleError(context, "target marker readback failed", { cause });
  }
  const matches = matchesFor(record.root, record.selector, context, true);
  if (!ownsMarker || matches.length !== 1 || matches[0] !== record.target) {
    throw new OutputTargetStaleError(context, "target marker is absent or no longer unique");
  }
  if (current.adopted.filter((sheet) => sheet === record.liveSheet).length !== 1) {
    throw new OutputTargetStaleError(context, "live output stylesheet was detached or duplicated");
  }
  if (sheetText(record.liveSheet, context, true) !== expectedText) {
    throw new OutputTargetStaleError(context, "live output stylesheet changed outside its sink");
  }
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
    const value = descriptor.value;
    if (value.length === 0) {
      throw new OutputBindingError(
        "OUTPUT_VALUES_INVALID",
        context,
        `vars['${key}'] must not be empty`,
      );
    }
    values.set(key, value);
  }
  return values;
}

function mergedValues(record, changedLease, changedValues, disposedLease) {
  const merged = new Map();
  for (const lease of record.leases) {
    if (!lease.active || lease === disposedLease) continue;
    const values = lease === changedLease ? changedValues : lease.values;
    for (const binding of lease.bindings) {
      if (values.has(binding)) merged.set(binding, values.get(binding));
    }
  }
  return new Map([...merged].sort(([left], [right]) => compareStrings(left, right)));
}

function prepareText(record, values, context) {
  if (values.size === 0) return "";
  const rule = replaceScratch(record.scratchSheet, record.selector, context);
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
  for (let index = 0; index < rule.style.length; index++) {
    readNames.push(rule.style.item(index));
  }
  const expectedNames = [...values.keys()];
  if (
    readNames.length !== expectedNames.length ||
    readNames.some((name, index) => name !== expectedNames[index])
  ) {
    throw new OutputStylesheetValidationError(
      context,
      "scratch CSSOM declaration set differs from the owned binding set",
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
  if (typeof text !== "string" || text.length === 0 || sheetText(record.scratchSheet, context) !== text) {
    throw new OutputStylesheetValidationError(
      context,
      "scratch CSSOM did not provide an exact complete stylesheet readback",
    );
  }
  return text;
}

function ownsNow(owns, context) {
  if (typeof owns !== "function") {
    throw new TypeError(`${contextLabel(context)}: owns must be a function`);
  }
  return owns() === true;
}

function detachDormant(record, context, recovered) {
  const failures = [];
  try {
    const current = Array.from(record.root.adoptedStyleSheets);
    record.root.adoptedStyleSheets = current.filter((sheet) => sheet !== record.liveSheet);
    if (Array.from(record.root.adoptedStyleSheets).includes(record.liveSheet)) {
      throw new Error("adoptedStyleSheets retained the dormant sheet");
    }
  } catch (cause) {
    failures.push(cause);
  }
  try {
    record.target.removeAttribute(record.markerName);
    if (record.target.hasAttribute(record.markerName)) {
      throw new Error("target retained the dormant sink marker");
    }
  } catch (cause) {
    failures.push(cause);
  }
  record.attached = false;
  if (failures.length > 0) {
    record.poisoned = true;
    throw new OutputAtomicityViolationError(
      context,
      "last-lease cleanup could not detach the dormant output sink",
      { cause: new AggregateError(failures, "output sink cleanup failures") },
    );
  }
  record.poisoned = !recovered;
}

function publish(record, lease, rawVars, owns, context) {
  if (!lease.active) return false;
  if (record.preparing || record.committing) throw new OutputSinkBusyError(context);
  if (typeof owns !== "function") {
    throw new TypeError(`${contextLabel(context)}: owns must be a function`);
  }
  const epoch = record.epoch;
  record.preparing = true;
  let text;
  let nextValues = null;
  try {
    if (!ownsNow(owns, context)) return false;
    assertOperational(record, context);
    if (!lease.active || record.epoch !== epoch) return false;
    nextValues = materializeVars(rawVars, lease.bindingSet, context);
    const merged = mergedValues(record, lease, nextValues, null);
    text = prepareText(record, merged, context);
    if (!ownsNow(owns, context) || !lease.active || record.epoch !== epoch) return false;
    assertFresh(record, context);
    if (record.stamp === Number.MAX_SAFE_INTEGER || record.epoch === Number.MAX_SAFE_INTEGER) {
      throw new OutputTargetStaleError(context, "target sink generation space is exhausted");
    }
    record.committing = true;
  } finally {
    record.preparing = false;
  }
  if (!record.committing) return false;
  let replaced = false;
  try {
    record.liveSheet.replaceSync(text);
    replaced = true;
    try {
      assertFresh(record, context, text);
    } catch (cause) {
      record.poisoned = true;
      throw new OutputAtomicityViolationError(
        context,
        "target host state changed during live stylesheet replacement",
        { cause },
      );
    }

    lease.values = nextValues;
    record.liveText = text;
    record.stamp++;
    record.epoch++;
    return true;
  } catch (cause) {
    if (replaced) throw cause;
    let actual;
    try {
      actual = sheetText(record.liveSheet, context, true);
    } catch (readCause) {
      record.poisoned = true;
      throw new OutputAtomicityViolationError(
        context,
        "host replacement failed and prior live bytes cannot be verified",
        { cause: readCause },
      );
    }
    if (actual !== record.liveText) {
      record.poisoned = true;
      throw new OutputAtomicityViolationError(
        context,
        "host replacement failure changed the prior live stylesheet",
        { cause },
      );
    }
    throw cause;
  } finally {
    record.committing = false;
  }
}

function revoke(record, lease, owns, context) {
  if (!lease.active) return false;
  if (record.preparing || record.committing) throw new OutputSinkBusyError(context);
  if (typeof owns !== "function") {
    throw new TypeError(`${contextLabel(context)}: owns must be a function`);
  }

  const epoch = record.epoch;
  record.preparing = true;
  let text;
  let preparationFailure = null;
  try {
    if (!ownsNow(owns, context)) return false;
    const merged = mergedValues(record, null, null, lease);
    try {
      text = prepareText(record, merged, context);
    } catch (cause) {
      preparationFailure = cause;
    }
    if (!ownsNow(owns, context) || !lease.active || record.epoch !== epoch) return false;
    record.committing = true;
  } finally {
    record.preparing = false;
  }
  if (!record.committing) return false;

  const failures = preparationFailure === null ? [] : [preparationFailure];
  let exact = false;
  try {
    if (preparationFailure !== null) {
      record.poisoned = true;
    } else {
      let before;
      let readable = false;
      try {
        before = sheetText(record.liveSheet, context, true);
        readable = true;
      } catch (cause) {
        failures.push(cause);
      }

      if (readable && before === text) {
        exact = true;
      } else {
        try {
          record.liveSheet.replaceSync(text);
        } catch (cause) {
          failures.push(cause);
        }
        try {
          exact = sheetText(record.liveSheet, context, true) === text;
        } catch (cause) {
          failures.push(cause);
        }
      }
      if (!exact) {
        failures.push(new Error("revoked output stylesheet did not match its remaining owners"));
        record.poisoned = true;
      }
    }

    const lastLease = record.leases.size === 1;
    for (const binding of lease.bindings) record.owners.delete(binding);
    record.leases.delete(lease);
    lease.values = new Map();
    lease.active = false;
    if (exact) record.liveText = text;
    if (record.stamp < Number.MAX_SAFE_INTEGER) record.stamp++;
    if (record.epoch < Number.MAX_SAFE_INTEGER) record.epoch++;

    if (lastLease) {
      try {
        detachDormant(record, context, exact);
      } catch (cause) {
        failures.push(cause);
      }
    }

    if (failures.length > 0) {
      record.poisoned = true;
      throw new OutputAtomicityViolationError(
        context,
        "output ownership was revoked but host cleanup could not be proved exact",
        { cause: new AggregateError(failures, "output revocation failures") },
      );
    }
    return true;
  } finally {
    record.committing = false;
  }
}

function commit(record, lease, rawVars, disposing, owns, context) {
  return disposing
    ? revoke(record, lease, owns, context)
    : publish(record, lease, rawVars, owns, context);
}

function leaseHandle(record, state, context) {
  return Object.freeze({
    outputBindings: state.bindings,
    publish(vars, owns = ALWAYS_OWNS) {
      if (!state.active) return false;
      if (record.preparing || record.committing) throw new OutputSinkBusyError(context);
      return commit(record, state, vars, false, owns, context);
    },
    dispose(owns = ALWAYS_OWNS) {
      if (!state.active) return false;
      if (record.preparing || record.committing) throw new OutputSinkBusyError(context);
      return commit(record, state, null, true, owns, context);
    },
    get stamp() {
      return record.stamp;
    },
    get state() {
      return state.active ? "active" : "disposed";
    },
  });
}

function acquireOutputLeaseUnchecked(target, outputBindings, context) {
  const descriptor = describeTarget(target, context);
  const bindings = materializeBindings(outputBindings, context);
  validateBindingNames(descriptor.Sheet, bindings, context);
  const inlineConflicts = inlineBindingConflicts(target, bindings, context);
  if (inlineConflicts.length > 0) {
    throw new OutputInlineBindingConflictError(context, inlineConflicts);
  }

  const existingDescriptor = Object.getOwnPropertyDescriptor(target, TARGET_STATE);
  if (existingDescriptor?.value?.protocol === PROTOCOL) {
    const existing = existingDescriptor.value;
    if (existing.preparing || existing.committing) throw new OutputSinkBusyError(context);
  }
  const record = recordFor(target, descriptor, context);
  if (record.preparing || record.committing) throw new OutputSinkBusyError(context);

  const conflicts = bindings.filter((binding) => record.owners.has(binding));
  if (conflicts.length > 0) throw new OutputBindingConflictError(context, conflicts);
  if (record.epoch === Number.MAX_SAFE_INTEGER) {
    throw new OutputTargetStaleError(context, "target sink generation space is exhausted");
  }

  const state = {
    bindings,
    bindingSet: new Set(bindings),
    values: new Map(),
    active: true,
  };
  record.leases.add(state);
  for (const binding of bindings) record.owners.set(binding, state);
  record.epoch++;
  return leaseHandle(record, state, context);
}

/**
 * Acquire a linear owner for an exact custom-property output set.
 *
 * This is an internal imperative-shell API. The target remains the caller's
 * element; a constructed stylesheet is adopted into its current Document or
 * ShadowRoot and is bound back to that element by a private marker selector.
 */
export function acquireOutputLease(target, outputBindings, context = "output sink") {
  if (target === null || (typeof target !== "object" && typeof target !== "function")) {
    return acquireOutputLeaseUnchecked(target, outputBindings, context);
  }

  const targets = acquisitionTargets(target, context);
  if (targets.has(target)) throw new OutputSinkBusyError(context);
  targets.add(target);
  try {
    return acquireOutputLeaseUnchecked(target, outputBindings, context);
  } finally {
    targets.delete(target);
  }
}
